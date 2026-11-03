use gio::prelude::*;
use std::{collections::BTreeMap, path::PathBuf};

pub type Aliases = BTreeMap<String, String>;

fn path() -> PathBuf {
    glib::user_config_dir().join("kvmon/connections.json")
}

pub fn load() -> Result<Aliases, String> {
    match std::fs::read(path()) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Aliases::new()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn name(aliases: &Aliases, computer: &kvmon_core::Computer) -> String {
    aliases
        .get(&computer.input.0.to_string())
        .cloned()
        .unwrap_or_else(|| computer.port.into())
}

pub fn update(aliases: &mut Aliases, input: u16, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        aliases.remove(&input.to_string());
    } else {
        aliases.insert(input.to_string(), text.into());
    }
}

pub fn save(input: u16, text: &str) -> Result<Aliases, String> {
    // Re-read before saving; never overwrite an unreadable or malformed file.
    let mut aliases = load()?;
    update(&mut aliases, input, text);
    let path = path();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&aliases).map_err(|e| e.to_string())?;
    gio::File::for_path(path)
        .replace_contents(
            &bytes,
            None,
            false,
            gio::FileCreateFlags::PRIVATE,
            gio::Cancellable::NONE,
        )
        .map_err(|e| e.to_string())?;
    Ok(aliases)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alias_roundtrip_and_empty_restores_default() {
        let computer = &kvmon_core::DELL_U4025QW.computers[0];
        let mut aliases = Aliases::new();
        update(&mut aliases, computer.input.0, "  Work laptop  ");
        let mut aliases: Aliases =
            serde_json::from_str(&serde_json::to_string(&aliases).unwrap()).unwrap();
        assert_eq!(name(&aliases, computer), "Work laptop");
        update(&mut aliases, computer.input.0, " \t ");
        assert_eq!(name(&aliases, computer), computer.port);
        assert!(aliases.is_empty());
    }
}
