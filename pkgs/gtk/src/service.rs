use crate::backend::Ddcutil;
use glib::variant::ToVariant;
use kvmon_core::{DELL_U4025QW, Input, MonitorBackend, QUIT, SWITCHING, State};
use std::{cell::RefCell, rc::Rc, sync::mpsc, thread, time::Duration};

const NAME: &str = "io.github.kvmon.Monitor";
const PATH: &str = "/io/github/kvmon/Monitor";

enum Work {
    Refresh,
    Select(Input),
}

struct Service {
    display: Option<kvmon_core::Display>,
    aliases: crate::aliases::Aliases,
    state: State,
    busy: bool,
    pending: Option<Input>,
    work: mpsc::Sender<Work>,
    connection: Option<gio::DBusConnection>,
}

impl Service {
    fn snapshot(&self) -> glib::Variant {
        let (status, message, input) = match &self.state {
            State::Checking => ("checking", "Loading…".to_owned(), 0),
            State::Switching => ("switching", SWITCHING.to_owned(), 0),
            State::Unavailable(error) => ("unavailable", error.clone(), 0),
            State::Ready(input) => (
                "ready",
                DELL_U4025QW
                    .computers
                    .iter()
                    .find(|c| c.input == *input)
                    .map(|c| crate::aliases::name(&self.aliases, c))
                    .unwrap_or_else(|| format!("Unconfigured input (0x{:02x})", input.0)),
                u32::from(input.0),
            ),
        };
        let computers: Vec<_> = DELL_U4025QW
            .computers
            .iter()
            .map(|c| (crate::aliases::name(&self.aliases, c), u32::from(c.input.0)))
            .collect();
        (status, message, input, computers, QUIT).to_variant()
    }

    fn publish(&self) {
        if let Some(connection) = &self.connection
            && let Err(error) =
                connection.emit_signal(None, PATH, NAME, "Changed", Some(&self.snapshot()))
        {
            eprintln!("Cannot publish monitor state: {error}");
        }
    }

    fn request(&mut self, work: Work) -> Result<(), &'static str> {
        if self.busy {
            match work {
                Work::Refresh => return Ok(()),
                Work::Select(input) if self.state.checked(input) => return Ok(()),
                Work::Select(input)
                    if self.state.can_select(input)
                        && DELL_U4025QW.computers.iter().any(|c| c.input == input) =>
                {
                    // Finish the in-flight read before performing the user's selection.
                    self.pending = Some(input);
                    self.state = State::Switching;
                    self.publish();
                    return Ok(());
                }
                _ => return Err("Monitor is busy"),
            }
        }
        if let Work::Select(input) = work {
            if !DELL_U4025QW.computers.iter().any(|c| c.input == input) {
                return Err("Input is not configured for this monitor");
            }
            if self.state.checked(input) {
                return Ok(());
            }
            if !self.state.can_select(input) {
                return Err("Monitor is not ready");
            }
            self.state = State::Switching;
        }
        self.work.send(work).map_err(|_| "Monitor worker stopped")?;
        self.busy = true;
        self.publish();
        Ok(())
    }
}

pub fn run() {
    let main_loop = glib::MainLoop::new(None, false);
    let (work_tx, work_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut backend = Ddcutil::new();
        while let Ok(work) = work_rx.recv() {
            // Rediscover every operation, including writes: bus numbers can change on reconnect.
            let mut discovered = None;
            let result = backend.target().and_then(|display| {
                discovered = Some(display.clone());
                let observed = backend.read_input(&display)?;
                match work {
                    Work::Refresh => Ok(observed),
                    Work::Select(input) => {
                        kvmon_core::switch(&mut backend, &display, &State::Ready(observed), input)
                    }
                }
            });
            if result_tx
                .send((
                    discovered,
                    match result {
                        Ok(input) => State::Ready(input),
                        Err(error) => State::Unavailable(error.to_string()),
                    },
                ))
                .is_err()
            {
                break;
            }
        }
    });
    let service = Rc::new(RefCell::new(Service {
        display: None,
        aliases: crate::aliases::load().unwrap_or_else(|error| {
            eprintln!("Cannot load connection aliases: {error}");
            Default::default()
        }),
        state: State::Checking,
        busy: false,
        pending: None,
        work: work_tx,
        connection: None,
    }));
    let bus_service = service.clone();
    let quit_loop = main_loop.clone();
    let lost_loop = main_loop.clone();
    let owner = gio::bus_own_name(
        gio::BusType::Session,
        NAME,
        gio::BusNameOwnerFlags::NONE,
        move |connection, _| {
            let info = gio::DBusNodeInfo::for_xml(include_str!("../../gnome-ext/interface.xml"))
                .expect("valid bundled D-Bus interface")
                .lookup_interface(NAME)
                .unwrap();
            let handler = bus_service.clone();
            let quit = quit_loop.clone();
            connection
                .register_object(PATH, &info)
                .method_call(move |_, _, _, _, method, params, invocation| {
                    let mut service = handler.borrow_mut();
                    let result = match method {
                        "GetMonitor" => {
                            let (name, bus) = service
                                .display
                                .as_ref()
                                .map(|d| (d.name.as_str(), d.id.as_str()))
                                .unwrap_or(("", ""));
                            invocation.return_value(Some(&(name, bus).to_variant()));
                            return;
                        }
                        "GetState" => {
                            invocation.return_value(Some(&service.snapshot()));
                            return;
                        }
                        "Refresh" => service.request(Work::Refresh),
                        "Settings" => {
                            let result = std::env::current_exe().and_then(|exe| {
                                std::process::Command::new(exe).arg("--settings").spawn()
                            });
                            match result {
                                Ok(mut child) => {
                                    thread::spawn(move || {
                                        let _ = child.wait();
                                    });
                                }
                                Err(error) => {
                                    invocation.return_dbus_error(
                                        "io.github.kvmon.Error",
                                        &error.to_string(),
                                    );
                                    return;
                                }
                            }
                            Ok(())
                        }
                        "SetAlias" => {
                            let Some((input, text)) = params.get::<(u32, String)>() else {
                                invocation
                                    .return_dbus_error("io.github.kvmon.Error", "Invalid alias");
                                return;
                            };
                            if !DELL_U4025QW
                                .computers
                                .iter()
                                .any(|c| u32::from(c.input.0) == input)
                            {
                                invocation.return_dbus_error(
                                    "io.github.kvmon.Error",
                                    "Unknown connection",
                                );
                                return;
                            }
                            match crate::aliases::save(input as u16, &text) {
                                Ok(aliases) => {
                                    service.aliases = aliases;
                                    service.publish();
                                    Ok(())
                                }
                                Err(error) => {
                                    invocation.return_dbus_error("io.github.kvmon.Error", &error);
                                    return;
                                }
                            }
                        }
                        "Select" => params
                            .get::<(u32,)>()
                            .and_then(|(input,)| u16::try_from(input).ok())
                            .ok_or("Invalid input")
                            .and_then(|input| service.request(Work::Select(Input(input)))),
                        "Quit" => {
                            quit.quit();
                            Ok(())
                        }
                        _ => Err("Unknown method"),
                    };
                    match result {
                        Ok(()) => invocation.return_value(Some(&().to_variant())),
                        Err(error) => invocation.return_dbus_error("io.github.kvmon.Error", error),
                    }
                })
                .build()
                .expect("register monitor D-Bus object");
            let mut service = bus_service.borrow_mut();
            service.connection = Some(connection);
            let _ = service.request(Work::Refresh);
        },
        |_, _| {},
        move |_, _| {
            eprintln!("Cannot own {NAME}; another instance may be running");
            lost_loop.quit();
        },
    );
    let results_service = service.clone();
    let results = glib::timeout_add_local(Duration::from_millis(50), move || {
        if let Ok((display, state)) = result_rx.try_recv() {
            let mut service = results_service.borrow_mut();
            service.display = display;
            service.state = state;
            service.busy = false;
            if let Some(input) = service.pending.take() {
                let _ = service.request(Work::Select(input));
            }
            service.publish();
        }
        glib::ControlFlow::Continue
    });
    let refresh = glib::timeout_add_local(Duration::from_secs(10), move || {
        let _ = service.borrow_mut().request(Work::Refresh);
        glib::ControlFlow::Continue
    });
    main_loop.run();
    refresh.remove();
    results.remove();
    gio::bus_unown_name(owner);
}
