use kvmon_core::{DELL_U4025QW, Display, Error, Input, MonitorBackend};
use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct Ddcutil {
    executable: PathBuf,
}

impl Ddcutil {
    pub fn new() -> Self {
        Self {
            executable: std::env::var_os("KVMON_DDCUTIL")
                .map(PathBuf::from)
                .unwrap_or_else(|| "ddcutil".into()),
        }
    }

    fn command(&self, args: &[&str]) -> Result<String, Error> {
        let mut child = Command::new(&self.executable)
            .args(args)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error(format!("Cannot run ddcutil: {e}. Install ddcutil.")))?;
        fn drain(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes)?;
            Ok(bytes)
        }
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out = thread::spawn(move || drain(stdout));
        let err = thread::spawn(move || drain(stderr));
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if start.elapsed() < Duration::from_secs(8) => {
                    thread::sleep(Duration::from_millis(25));
                }
                other => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(Error(match other {
                        Err(e) => format!("Cannot wait for ddcutil: {e}"),
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
        if !status?.success() {
            return Err(Error(format!(
                "ddcutil failed: {}{}. Check access to /dev/i2c-* (see README).",
                String::from_utf8_lossy(&stdout).trim(),
                String::from_utf8_lossy(&stderr).trim(),
            )));
        }
        String::from_utf8(stdout).map_err(|e| Error(e.to_string()))
    }

    pub fn target(&mut self) -> Result<Display, Error> {
        let serial = std::env::var("KVMON_DISPLAY_SERIAL").ok();
        let output = self.command(&["detect"])?;
        let mut displays = parse_displays(&output, serial.as_deref())?.into_iter();
        let display = displays
            .next()
            .ok_or_else(|| Error("Dell U4025QW is unavailable".into()))?;
        if displays.next().is_some() {
            return Err(Error(
                "Multiple Dell monitors: set KVMON_DISPLAY_SERIAL".into(),
            ));
        }
        Ok(display)
    }
}

impl MonitorBackend for Ddcutil {
    fn discover(&mut self) -> Result<Vec<Display>, Error> {
        parse_displays(&self.command(&["detect"])?, None)
    }

    fn read_input(&mut self, display: &Display) -> Result<Input, Error> {
        parse_input(&self.command(&["--bus", &display.id, "--terse", "getvcp", "60"])?)
    }

    fn select_input(&mut self, display: &Display, input: Input) -> Result<(), Error> {
        if !DELL_U4025QW.computers.iter().any(|c| c.input == input) {
            return Err(Error("Input is not configured for this monitor".into()));
        }
        self.command(&[
            "--bus",
            &display.id,
            "setvcp",
            "60",
            &format!("0x{:02x}", input.0),
        ])?;
        thread::sleep(Duration::from_millis(500));
        Ok(())
    }
}

fn parse_displays(output: &str, serial: Option<&str>) -> Result<Vec<Display>, Error> {
    let mut displays = Vec::new();
    // Only numbered, valid displays are candidates; never use an arbitrary I2C bus.
    for block in output
        .split("\nDisplay ")
        .map(|s| s.strip_prefix("Display ").unwrap_or(s))
    {
        if !block
            .lines()
            .next()
            .is_some_and(|s| s.trim().parse::<u32>().is_ok())
        {
            continue;
        }
        let field = |key: &str| {
            block
                .lines()
                .find_map(|line| line.trim().strip_prefix(key).map(str::trim))
        };
        if field("Model:") != Some(DELL_U4025QW.model)
            || !field("Mfg id:").is_some_and(|s| s.split_whitespace().next() == Some("DEL"))
            || serial.is_some_and(|s| field("Serial number:") != Some(s))
        {
            continue;
        }
        let bus = field("I2C bus:")
            .and_then(|s| s.strip_prefix("/dev/i2c-"))
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| Error("Missing I2C bus in ddcutil display discovery".into()))?;
        displays.push(Display {
            id: bus.into(),
            name: DELL_U4025QW.model.into(),
        });
    }
    Ok(displays)
}

fn parse_input(output: &str) -> Result<Input, Error> {
    for line in output.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if let ["VCP", "60", "SNC", value] = fields.as_slice() {
            let value = value.strip_prefix("x").or_else(|| value.strip_prefix("0x"));
            if let Some(value) = value.and_then(|s| u16::from_str_radix(s, 16).ok()) {
                let low = value & 0xff;
                let high = value >> 8;
                if low != 0 && (high == 0 || high == low) {
                    return Ok(Input(low));
                }
            }
        }
    }
    Err(Error(format!("Unexpected input reply: {}", output.trim())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_matches_identity_and_serial_not_bus_order() {
        let fixture = include_str!("../tests/fixtures/detect.txt");
        let displays = parse_displays(fixture, None).unwrap();
        assert_eq!(displays.len(), 2);
        assert_eq!(displays[0].id, "8");
        assert_eq!(parse_displays(fixture, Some("SECOND")).unwrap()[0].id, "12");
        assert!(parse_displays(fixture, Some("missing")).unwrap().is_empty());
        assert!(parse_displays("", None).unwrap().is_empty());
        assert!(
            parse_displays(
                "Display 1\n Mfg id: DEL\n Model: DELL U4025QW\n I2C bus: garbage",
                None
            )
            .is_err()
        );
    }

    #[test]
    fn input_requires_a_valid_noncontinuous_vcp_reply() {
        for (reply, value) in [
            ("VCP 60 SNC x0f", 15),
            ("VCP 60 SNC x1919", 25),
            ("VCP 60 SNC x11", 17),
        ] {
            assert_eq!(parse_input(reply), Ok(Input(value)));
        }
        for reply in [
            "VCP 60 ERR",
            "VCP 10 SNC x19",
            "VCP 60 SNC x00",
            "VCP 60 SNC x0119",
            "VCP 60 C 25 255",
            "permission denied",
        ] {
            assert!(parse_input(reply).is_err(), "{reply}");
        }
    }
}
