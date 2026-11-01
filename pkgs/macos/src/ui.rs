use crate::backend::M1ddc;
use kvmon_core::{CHECKING, DELL_U4025QW, Input, MonitorBackend, QUIT, SWITCHING, State};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject, sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSControlStateValueOff, NSControlStateValueOn,
    NSEventTrackingRunLoopMode, NSMenu, NSMenuDelegate, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{
    NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes, NSString, NSTimer,
};
use std::{
    cell::RefCell,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

enum Request {
    Refresh,
    Select(Input),
}

struct Model {
    state: State,
    busy: bool,
    last_refresh: Instant,
}

struct Ivars {
    items: Vec<Retained<NSMenuItem>>,
    message: Retained<NSMenuItem>,
    status: Retained<NSStatusItem>,
    model: RefCell<Model>,
    requests: mpsc::Sender<Request>,
    results: mpsc::Receiver<State>,
}

define_class!(
    // All AppKit objects and mutable UI state stay on the main thread.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct Controller;

    unsafe impl NSObjectProtocol for Controller {}

    unsafe impl NSMenuDelegate for Controller {
        #[unsafe(method(menuWillOpen:))]
        fn menu_will_open(&self, _menu: &NSMenu) {
            self.request(Request::Refresh);
        }
    }

    impl Controller {
        #[unsafe(method(selectComputer:))]
        fn select_computer(&self, sender: &NSMenuItem) {
            if let Some(computer) = DELL_U4025QW.computers.get(sender.tag() as usize)
                && self.ivars().model.borrow().state.can_select(computer.input)
            {
                self.request(Request::Select(computer.input));
            }
        }

        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            while let Ok(state) = self.ivars().results.try_recv() {
                let mut model = self.ivars().model.borrow_mut();
                model.state = state;
                model.busy = false;
                model.last_refresh = Instant::now();
                drop(model);
                self.render();
            }
            if self.ivars().model.borrow().last_refresh.elapsed() >= Duration::from_secs(10) {
                self.request(Request::Refresh);
            }
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: &NSMenuItem) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

impl Controller {
    fn request(&self, request: Request) {
        let mut model = self.ivars().model.borrow_mut();
        if model.busy {
            return;
        }
        model.busy = true;
        model.state = match request {
            Request::Refresh => State::Checking,
            Request::Select(_) => State::Switching,
        };
        if self.ivars().requests.send(request).is_err() {
            model.state = State::Unavailable("Monitor worker stopped; restart kvmon".into());
            model.busy = false;
            model.last_refresh = Instant::now();
        }
        drop(model);
        self.render();
    }

    fn render(&self) {
        let model = self.ivars().model.borrow();
        // Put the observed active computer first without rebuilding the tracked menu.
        let mut computers: Vec<_> = DELL_U4025QW.computers.iter().enumerate().collect();
        computers.sort_by_key(|(_, computer)| !model.state.checked(computer.input));
        for (item, (index, computer)) in self.ivars().items.iter().zip(computers) {
            item.setTitle(&NSString::from_str(computer.name));
            item.setTag(index as isize);
            item.setState(if model.state.checked(computer.input) {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
            item.setEnabled(matches!(model.state, State::Ready(_)) && !model.busy);
        }
        let message = match &model.state {
            State::Checking => Some(CHECKING.into()),
            State::Switching => Some(SWITCHING.into()),
            State::Unavailable(error) => Some(error.clone()),
            State::Ready(input) if !DELL_U4025QW.computers.iter().any(|c| c.input == *input) => {
                Some(format!("Current input: 0x{:02x}", input.0))
            }
            State::Ready(_) => None,
        };
        self.ivars().message.setHidden(message.is_none());
        if let Some(message) = message {
            let short: String = message.chars().take(90).collect();
            self.ivars().message.setTitle(&NSString::from_str(&short));
            self.ivars()
                .message
                .setToolTip(Some(&NSString::from_str(&message)));
        }
        if let Some(button) = self.ivars().status.button(self.mtm()) {
            button.setToolTip(Some(&NSString::from_str("kvmon — Dell U4025QW")));
        }
    }
}

pub fn run() {
    let mtm = MainThreadMarker::new().expect("AppKit must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    let status = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    if let Some(button) = status.button(mtm) {
        button.setTitle(&NSString::from_str("KVM"));
    }
    status.setMenu(Some(&menu));
    let items: Vec<_> = DELL_U4025QW
        .computers
        .iter()
        .map(|_| {
            let item = NSMenuItem::new(mtm);
            menu.addItem(&item);
            item
        })
        .collect();
    let message = NSMenuItem::new(mtm);
    message.setEnabled(false);
    menu.addItem(&message);
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    let quit = NSMenuItem::new(mtm);
    quit.setTitle(&NSString::from_str(QUIT));
    quit.setKeyEquivalent(&NSString::from_str("q"));
    menu.addItem(&quit);

    let (requests, incoming) = mpsc::channel();
    let (outgoing, results) = mpsc::channel();
    thread::spawn(move || {
        let mut backend = M1ddc::new();
        while let Ok(request) = incoming.recv() {
            let result = backend.target().and_then(|display| {
                let current = backend.read_input(&display)?;
                match request {
                    Request::Refresh => Ok(current),
                    Request::Select(input) => {
                        kvmon_core::switch(&mut backend, &display, &State::Ready(current), input)
                    }
                }
            });
            let state = match result {
                Ok(input) => State::Ready(input),
                Err(error) => {
                    eprintln!("kvmon: {error}");
                    State::Unavailable(error.to_string())
                }
            };
            if outgoing.send(state).is_err() {
                break;
            }
        }
    });
    let controller = Controller::alloc(mtm).set_ivars(Ivars {
        items,
        message,
        status,
        model: RefCell::new(Model {
            state: State::Checking,
            busy: false,
            last_refresh: Instant::now(),
        }),
        requests,
        results,
    });
    // SAFETY: NSObject initialization; targets implement the selectors below and
    // controller is retained for the lifetime of the menu and timer.
    let controller: Retained<Controller> = unsafe { msg_send![super(controller), init] };
    unsafe {
        for item in &controller.ivars().items {
            item.setTarget(Some(&controller));
            item.setAction(Some(sel!(selectComputer:)));
        }
        quit.setTarget(Some(&controller));
        quit.setAction(Some(sel!(quit:)));
    }
    menu.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
    // SAFETY: tick: has NSTimer's required target/selector signature. The run loop
    // invokes it only on this main thread, including while a menu is tracking.
    let timer = unsafe {
        NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
            0.1,
            &controller,
            sel!(tick:),
            None,
            true,
        )
    };
    let run_loop = NSRunLoop::mainRunLoop();
    unsafe {
        run_loop.addTimer_forMode(&timer, NSRunLoopCommonModes);
        run_loop.addTimer_forMode(&timer, NSEventTrackingRunLoopMode);
    }
    controller.request(Request::Refresh);
    app.run();
    timer.invalidate();
    menu.setDelegate(None);
}
