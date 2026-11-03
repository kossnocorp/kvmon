#[cfg(target_os = "linux")]
mod aliases;
#[cfg(target_os = "linux")]
mod backend;
#[cfg(target_os = "linux")]
mod service;
#[cfg(target_os = "linux")]
mod settings;

#[cfg(target_os = "linux")]
fn main() {
    if std::env::args().any(|arg| arg == "--settings") {
        settings::run();
        return;
    }
    if std::env::args().any(|arg| arg == "--probe") {
        use kvmon_core::MonitorBackend;
        let mut backend = backend::Ddcutil::new();
        match backend.target().and_then(|display| {
            let input = backend.read_input(&display)?;
            println!(
                "{} ({}) — input 0x{:02x}",
                display.name, display.id, input.0
            );
            Ok(())
        }) {
            Ok(()) => (),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    } else {
        service::run();
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("kvmon-gtk requires Linux");
    std::process::exit(1);
}
