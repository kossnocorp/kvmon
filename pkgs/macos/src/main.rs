#[cfg(target_os = "macos")]
mod backend;
#[cfg(target_os = "macos")]
mod ui;

#[cfg(target_os = "macos")]
fn main() {
    if std::env::args().any(|arg| arg == "--probe") {
        use kvmon_core::MonitorBackend;
        let mut backend = backend::M1ddc::new();
        let result = backend.target().and_then(|display| {
            let input = backend.read_input(&display)?;
            println!(
                "{} ({}) — input 0x{:02x}",
                display.name, display.id, input.0
            );
            Ok(())
        });
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
    } else {
        ui::run();
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("kvmon-macos requires macOS; use the GTK frontend on Linux.");
    std::process::exit(1);
}
