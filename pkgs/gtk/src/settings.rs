use adw::prelude::*;
use glib::variant::ToVariant;
use kvmon_core::DELL_U4025QW;
use std::{cell::RefCell, rc::Rc};

pub fn run() {
    let app = adw::Application::builder()
        .application_id("io.github.kvmon.Settings")
        .build();
    app.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let window = adw::PreferencesWindow::builder()
            .application(app).title("Kvmon Settings")
            .default_width(600).default_height(380).search_enabled(false).build();
        let page = adw::PreferencesPage::new();
        let monitor_group = adw::PreferencesGroup::new();
        let monitor = adw::ActionRow::builder().title("Checking monitor…")
            .use_markup(false).build();
        monitor.add_prefix(&gtk::Image::from_icon_name("video-display-symbolic"));
        monitor_group.add(&monitor);
        page.add(&monitor_group);
        let weak_monitor = monitor.downgrade();
        glib::spawn_future_local(async move {
            // Query the service's discovered identity, not the configured model label.
            while let Some(monitor) = weak_monitor.upgrade() {
                let result = async {
                    let bus = gio::bus_get_future(gio::BusType::Session).await?;
                    bus.call_future(Some("io.github.kvmon.Monitor"), "/io/github/kvmon/Monitor",
                        "io.github.kvmon.Monitor", "GetMonitor", None, None,
                        gio::DBusCallFlags::NONE, 5000).await
                }.await;
                match result.ok().and_then(|v| v.get::<(String, String)>()) {
                    Some((name, bus)) if !name.is_empty() => {
                        monitor.set_title(&name);
                        monitor.set_subtitle(&format!("DDC/CI · /dev/i2c-{bus}"));
                    }
                    _ => {
                        monitor.set_title("Monitor unavailable");
                        monitor.set_subtitle("Connection aliases apply to the Dell U4025QW profile");
                    }
                }
                drop(monitor);
                glib::timeout_future_seconds(2).await;
            }
        });
        let group = adw::PreferencesGroup::builder()
            .title("Device Connections")
            .build();
        let aliases = match crate::aliases::load() {
            Ok(aliases) => aliases,
            Err(error) => {
                window.add_toast(adw::Toast::new(&format!("Cannot load names: {error}")));
                Default::default()
            }
        };
        for computer in DELL_U4025QW.computers {
            let saved = Rc::new(RefCell::new(aliases.get(&computer.input.0.to_string()).cloned().unwrap_or_default()));
            let row = adw::PreferencesRow::builder().activatable(false).build();
            let stack = gtk::Stack::builder().hexpand(true)
                .hhomogeneous(false).vhomogeneous(true).build();
            let display = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let labels = gtk::Box::new(gtk::Orientation::Vertical, 3);
            labels.set_hexpand(true);
            labels.set_valign(gtk::Align::Center);
            let title = gtk::Label::builder().label(crate::aliases::name(&aliases, computer))
                .xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
            let details = gtk::Label::builder()
                .label(format!("Input 0x{:02x}", computer.input.0))
                .xalign(0.0).css_classes(["dim-label", "caption"]).build();
            labels.append(&title);
            labels.append(&details);
            display.append(&labels);
            let edit = gtk::Button::builder().icon_name("document-edit-symbolic")
                .tooltip_text("Edit connection name").css_classes(["flat"])
                .valign(gtk::Align::Center).width_request(34).build();
            display.append(&edit);
            let editor = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let entry = gtk::Entry::builder().placeholder_text(computer.port)
                .hexpand(true).valign(gtk::Align::Center)
                .max_length(80).width_chars(1).build();
            entry.update_property(&[gtk::accessible::Property::Label("Connection name")]);
            let save = gtk::Button::builder().icon_name("object-select-symbolic")
                .tooltip_text("Save name").css_classes(["suggested-action"])
                .valign(gtk::Align::Center).width_request(34).build();
            let cancel = gtk::Button::builder().icon_name("edit-undo-symbolic")
                .tooltip_text("Cancel editing").css_classes(["flat"])
                .valign(gtk::Align::Center).width_request(34).build();
            editor.append(&entry);
            editor.append(&save);
            editor.append(&cancel);
            stack.add_named(&display, Some("display"));
            stack.add_named(&editor, Some("edit"));
            stack.set_margin_start(12);
            stack.set_margin_end(12);
            stack.set_margin_top(10);
            stack.set_margin_bottom(10);
            row.set_child(Some(&stack));

            let begin: Rc<dyn Fn()> = {
                let (stack, entry, saved) = (stack.clone(), entry.clone(), saved.clone());
                Rc::new(move || {
                    entry.set_text(&saved.borrow());
                    stack.set_visible_child_name("edit");
                    entry.grab_focus();
                    entry.select_region(0, -1);
                })
            };
            let begin_click = begin.clone();
            edit.connect_clicked(move |_| begin_click());
            let gesture = gtk::GestureClick::new();
            gesture.set_button(1);
            let stack_check = stack.clone();
            gesture.connect_released(move |_, presses, _, _| {
                if presses == 2 && stack_check.visible_child_name().as_deref() == Some("display") {
                    begin();
                }
            });
            row.add_controller(gesture);
            let cancel_edit: Rc<dyn Fn()> = {
                let (stack, edit) = (stack.clone(), edit.clone());
                Rc::new(move || { stack.set_visible_child_name("display"); edit.grab_focus(); })
            };
            let cancel_click = cancel_edit.clone();
            cancel.connect_clicked(move |_| cancel_click());
            let keys = gtk::EventControllerKey::new();
            keys.connect_key_pressed(move |_, key, _, _| {
                if key == gtk::gdk::Key::Escape {
                    cancel_edit();
                    glib::Propagation::Stop
                } else { glib::Propagation::Proceed }
            });
            entry.add_controller(keys);
            let window = window.downgrade();
            let row_weak = title.downgrade();
            let commit: Rc<dyn Fn()> = {
                let entry = entry.clone();
                Rc::new(move || {
                    if !editor.is_sensitive() { return; }
                    let text = entry.text().trim().to_owned();
                    editor.set_sensitive(false);
                    let (editor, stack, saved, window, row, edit) = (
                        editor.clone(), stack.clone(), saved.clone(), window.clone(), row_weak.clone(), edit.clone());
                    glib::spawn_future_local(async move {
                        let result = async {
                            let bus = gio::bus_get_future(gio::BusType::Session).await?;
                            bus.call_future(Some("io.github.kvmon.Monitor"),
                                "/io/github/kvmon/Monitor", "io.github.kvmon.Monitor", "SetAlias",
                                Some(&(u32::from(computer.input.0), &text).to_variant()),
                                None, gio::DBusCallFlags::NONE, 5000).await
                        }.await;
                        editor.set_sensitive(true);
                        match result {
                            Ok(_) => {
                                if let Some(row) = row.upgrade() {
                                    row.set_label(if text.is_empty() { computer.port } else { &text });
                                }
                                *saved.borrow_mut() = text;
                                stack.set_visible_child_name("display");
                                edit.grab_focus();
                            }
                            Err(error) => if let Some(window) = window.upgrade() {
                                window.add_toast(adw::Toast::new(&format!("Could not save name: {error}")));
                            },
                        }
                    });
                })
            };
            let activate_commit = commit.clone();
            entry.connect_activate(move |_| activate_commit());
            save.connect_clicked(move |_| commit());
            group.add(&row);
        }
        page.add(&group);
        let footer = adw::PreferencesGroup::new();
        let hint = gtk::Label::builder()
            .label("Double-click a connection or choose Edit to rename it. Clear the name to use its default.")
            .wrap(true).xalign(0.0).css_classes(["dim-label", "caption"]).build();
        footer.add(&hint);
        page.add(&footer);
        window.add(&page);
        window.present();
    });
    app.run_with_args::<&str>(&[]);
}
