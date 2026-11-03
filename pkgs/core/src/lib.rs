//! Platform-neutral monitor control and menu state.
use std::fmt;

pub const QUIT: &str = "Quit";
pub const CHECKING: &str = "Checking monitor…";
pub const SWITCHING: &str = "Switching input…";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Input(pub u16);

#[derive(Clone, Copy, Debug)]
pub struct Computer {
    pub name: &'static str,
    pub port: &'static str,
    pub input: Input,
}

pub struct MonitorProfile {
    pub model: &'static str,
    pub computers: &'static [Computer],
}

pub const DELL_U4025QW: MonitorProfile = MonitorProfile {
    model: "DELL U4025QW",
    computers: &[
        Computer {
            name: "MacBook",
            port: "Thunderbolt",
            input: Input(0x19),
        },
        Computer {
            name: "Fedora PC",
            port: "DisplayPort-1",
            input: Input(0x0f),
        },
    ],
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

pub trait MonitorBackend: Send {
    fn discover(&mut self) -> Result<Vec<Display>, Error>;
    fn read_input(&mut self, display: &Display) -> Result<Input, Error>;
    fn select_input(&mut self, display: &Display, input: Input) -> Result<(), Error>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Checking,
    Ready(Input),
    Switching,
    Unavailable(String),
}

impl State {
    pub fn checked(&self, input: Input) -> bool {
        *self == Self::Ready(input)
    }
    pub fn can_select(&self, input: Input) -> bool {
        matches!(self, Self::Ready(current) if *current != input)
    }
}

/// A successful write is not a confirmed input change. Always read back.
pub fn switch(
    backend: &mut impl MonitorBackend,
    display: &Display,
    current: &State,
    input: Input,
) -> Result<Input, Error> {
    if !current.can_select(input) {
        return match current {
            State::Ready(active) => Ok(*active),
            _ => Err(Error("Monitor is not ready".into())),
        };
    }
    backend.select_input(display, input)?;
    let observed = backend.read_input(display)?;
    if observed != input {
        return Err(Error("Monitor has not confirmed the selected input".into()));
    }
    Ok(observed)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        writes: usize,
        observed: Result<Input, Error>,
    }
    impl MonitorBackend for Fake {
        fn discover(&mut self) -> Result<Vec<Display>, Error> {
            Ok(vec![])
        }
        fn read_input(&mut self, _: &Display) -> Result<Input, Error> {
            self.observed.clone()
        }
        fn select_input(&mut self, _: &Display, _: Input) -> Result<(), Error> {
            self.writes += 1;
            Ok(())
        }
    }
    #[test]
    fn active_selection_is_noop_and_write_requires_confirmation() {
        let display = Display {
            id: "test".into(),
            name: "test".into(),
        };
        let mut backend = Fake {
            writes: 0,
            observed: Ok(Input(25)),
        };
        assert_eq!(
            switch(&mut backend, &display, &State::Ready(Input(25)), Input(25)),
            Ok(Input(25))
        );
        assert_eq!(backend.writes, 0);
        assert!(switch(&mut backend, &display, &State::Ready(Input(25)), Input(15)).is_err());
        backend.observed = Err(Error("Disconnected".into()));
        assert!(switch(&mut backend, &display, &State::Ready(Input(25)), Input(15)).is_err());
        assert!(!State::Switching.checked(Input(15)));
        backend.observed = Ok(Input(15));
        assert_eq!(
            switch(&mut backend, &display, &State::Ready(Input(25)), Input(15)),
            Ok(Input(15))
        );
    }
}
