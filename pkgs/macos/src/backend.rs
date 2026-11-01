use kvmon_core::{DELL_U4025QW, Display, Error, Input, MonitorBackend};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct M1ddc {
    executable: PathBuf,
}

impl M1ddc {
    pub fn new() -> Self {
        let executable = std::env::var_os("KVMON_M1DDC")
            .map(PathBuf::from)
            .or_else(|| {
                ["/opt/homebrew/bin/m1ddc", "/usr/local/bin/m1ddc"]
                    .into_iter()
                    .map(PathBuf::from)
                    .find(|path| path.is_file())
            })
            .unwrap_or_else(|| "m1ddc".into());
        Self { executable }
    }

    fn command(&self, args: &[&str]) -> Result<String, Error> {
        let mut child = Command::new(&self.executable)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                Error(format!(
                    "Cannot run m1ddc: {e}. Install with brew install m1ddc."
                ))
            })?;
        // Drain both pipes concurrently so unexpected output cannot deadlock the timeout.
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        fn drain(mut stream: impl std::io::Read) -> std::io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes)?;
            Ok(bytes)
        }
        let out = thread::spawn(move || drain(stdout));
        let err = thread::spawn(move || drain(stderr));
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if start.elapsed() < Duration::from_secs(5) => {
                    thread::sleep(Duration::from_millis(25))
                }
                other => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(Error(match other {
                        Err(e) => format!("Cannot wait for m1ddc: {e}"),
                        _ => "Monitor command timed out".into(),
                    }));
                }
            }
        };
        let stdout = out
            .join()
            .map_err(|_| Error("Output reader failed".into()))?
            .map_err(|e| Error(e.to_string()))?;
        let stderr = err
            .join()
            .map_err(|_| Error("Error reader failed".into()))?
            .map_err(|e| Error(e.to_string()))?;
        let status = status?;
        if !status.success() {
            return Err(Error(
                format!(
                    "m1ddc failed: {}{}",
                    String::from_utf8_lossy(&stdout),
                    String::from_utf8_lossy(&stderr)
                )
                .trim()
                .into(),
            ));
        }
        String::from_utf8(stdout).map_err(|e| Error(e.to_string()))
    }

    pub fn target(&mut self) -> Result<Display, Error> {
        let preferred = std::env::var("KVMON_DISPLAY_UUID").ok();
        let mut displays = self.discover()?.into_iter().filter(|display| {
            display.name == DELL_U4025QW.model
                && preferred
                    .as_ref()
                    .is_none_or(|id| display.id.eq_ignore_ascii_case(id))
        });
        let display = displays
            .next()
            .ok_or_else(|| Error("Dell U4025QW is unavailable".into()))?;
        if displays.next().is_some() {
            return Err(Error(
                "Multiple Dell monitors: set KVMON_DISPLAY_UUID".into(),
            ));
        }
        Ok(display)
    }
}

impl MonitorBackend for M1ddc {
    fn discover(&mut self) -> Result<Vec<Display>, Error> {
        parse_displays(&self.command(&["display", "list"])?)
    }
    fn read_input(&mut self, display: &Display) -> Result<Input, Error> {
        parse_input(&self.command(&["display", &display.id, "get", "input"])?)
    }
    fn select_input(&mut self, display: &Display, input: Input) -> Result<(), Error> {
        if !DELL_U4025QW
            .computers
            .iter()
            .any(|computer| computer.input == input)
        {
            return Err(Error("Input is not configured for this monitor".into()));
        }
        self.command(&["display", &display.id, "set", "input", &input.0.to_string()])?;
        thread::sleep(Duration::from_millis(500));
        Ok(())
    }
}

fn parse_displays(output: &str) -> Result<Vec<Display>, Error> {
    output
        .lines()
        .filter(|line| line.starts_with('['))
        .map(|line| {
            let (_, rest) = line
                .split_once("] ")
                .ok_or_else(|| Error("Unexpected display list".into()))?;
            let (name, id) = rest
                .rsplit_once(" (")
                .ok_or_else(|| Error("Missing display UUID".into()))?;
            let id = id
                .strip_suffix(')')
                .ok_or_else(|| Error("Malformed display UUID".into()))?;
            if id.len() != 36 || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                return Err(Error("Invalid display UUID".into()));
            }
            Ok(Display {
                id: id.into(),
                name: name.into(),
            })
        })
        .collect()
}

fn parse_input(output: &str) -> Result<Input, Error> {
    let value: u16 = output
        .trim()
        .parse()
        .map_err(|_| Error(format!("Unexpected input reply: {}", output.trim())))?;
    // U4025QW returned 0x1919 on Thunderbolt. VCP 0x60's selector is SL;
    // accept the observed duplicated byte as well as the standard zero SH.
    let low = value & 0xff;
    let high = value >> 8;
    if low == 0 || (high != 0 && high != low) {
        return Err(Error(format!("Invalid input reply: 0x{value:04x}")));
    }
    Ok(Input(low))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_real_display_list_without_selecting_unknown_display() {
        let displays = parse_displays("[1] DELL U4025QW (1D5ED280-77FB-4355-968B-A6CDCE617E83)\n[2] Unknown Display (37D8832A-2D66-02CA-B9F7-8F30A301B230)\n").unwrap();
        assert_eq!(displays.len(), 2);
        assert_eq!(displays[0].name, DELL_U4025QW.model);
        assert!(parse_displays("[1] DELL U4025QW (garbage)").is_err());
    }
    #[test]
    fn parses_dell_and_standard_replies_but_rejects_errors() {
        assert_eq!(parse_input("6425\n"), Ok(Input(25)));
        assert_eq!(parse_input("15\n"), Ok(Input(15)));
        assert_eq!(parse_input("17"), Ok(Input(17)));
        for bad in ["0", "-1", "65536", "Error", "Writing 15", "281"] {
            assert!(parse_input(bad).is_err(), "{bad}");
        }
    }
}
