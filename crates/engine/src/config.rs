//! Configuration in a small, documented TOML subset.
//!
//! Only flat `key = value` pairs under `[section]` headers are supported, with
//! string, boolean and integer values. That is all the config needs, and it
//! avoids pulling a full TOML implementation into the trust boundary.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::EngineError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Directory holding the database, identity file and spool.
    pub data_dir: PathBuf,
    /// Shared spool directory used by the file transport.
    pub spool_dir: PathBuf,
    pub profile_name: String,
    pub address: String,
    pub locale: String,
    pub color: String,
    pub density_following: String,
    pub density_lists: String,
    pub density_you: String,
    pub density_all: String,
    pub body_lines: i64,
    pub move_protocol_messages: bool,
    pub protocol_folder: String,
    pub auto_sync_minutes: i64,
    pub transport: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            data_dir: PathBuf::from("data"),
            spool_dir: PathBuf::from("spool"),
            profile_name: String::new(),
            address: String::new(),
            locale: "en".to_string(),
            color: "auto".to_string(),
            density_following: "cozy".to_string(),
            density_lists: "compact".to_string(),
            density_you: "cozy".to_string(),
            density_all: "compact".to_string(),
            body_lines: 6,
            move_protocol_messages: true,
            protocol_folder: "SocialMail".to_string(),
            auto_sync_minutes: 0,
            transport: "spool".to_string(),
        }
    }
}

impl Config {
    /// Load a config file, falling back to defaults when it does not exist.
    /// Relative paths are resolved against the config file's directory.
    pub fn load(path: &Path) -> Result<Self, EngineError> {
        let mut config = Config::default();
        if !path.exists() {
            config.data_dir = PathBuf::from("data");
            config.spool_dir = PathBuf::from("spool");
            return Ok(config);
        }
        let text = std::fs::read_to_string(path)?;
        let mut section = String::new();
        for (number, raw) in text.lines().enumerate() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                if !line.ends_with(']') {
                    return Err(EngineError::Config(format!(
                        "line {}: unterminated section header",
                        number + 1
                    )));
                }
                section = line[1..line.len() - 1].trim().to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(EngineError::Config(format!(
                    "line {}: expected `key = value`",
                    number + 1
                )));
            };
            let key = key.trim();
            let value = value.trim();
            let full = if section.is_empty() {
                key.to_string()
            } else {
                format!("{section}.{key}")
            };
            config.apply(&full, value, number + 1)?;
        }
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        config.data_dir = resolve(base, &config.data_dir);
        config.spool_dir = resolve(base, &config.spool_dir);
        Ok(config)
    }

    fn apply(&mut self, key: &str, value: &str, line: usize) -> Result<(), EngineError> {
        let bad = |expected: &str| {
            EngineError::Config(format!("line {line}: `{key}` expects {expected}"))
        };
        match key {
            "profile.name" => {
                self.profile_name = parse_string(value).ok_or_else(|| bad("a string"))?
            }
            "profile.address" => {
                self.address = parse_string(value).ok_or_else(|| bad("a string"))?
            }
            "profile.locale" => self.locale = parse_string(value).ok_or_else(|| bad("a string"))?,
            "ui.color" => self.color = parse_string(value).ok_or_else(|| bad("a string"))?,
            "ui.density_following" => {
                self.density_following = parse_string(value).ok_or_else(|| bad("a string"))?
            }
            "ui.density_lists" => {
                self.density_lists = parse_string(value).ok_or_else(|| bad("a string"))?
            }
            "ui.density_you" => {
                self.density_you = parse_string(value).ok_or_else(|| bad("a string"))?
            }
            "ui.density_all" => {
                self.density_all = parse_string(value).ok_or_else(|| bad("a string"))?
            }
            "ui.body_lines" => {
                self.body_lines = parse_int(value).ok_or_else(|| bad("an integer"))?
            }
            "mail.move_protocol_messages" => {
                self.move_protocol_messages =
                    parse_bool(value).ok_or_else(|| bad("true or false"))?
            }
            "mail.protocol_folder" => {
                self.protocol_folder = parse_string(value).ok_or_else(|| bad("a string"))?
            }
            "sync.auto_interval_minutes" => {
                self.auto_sync_minutes = parse_int(value).ok_or_else(|| bad("an integer"))?
            }
            "storage.data_dir" => {
                self.data_dir = PathBuf::from(parse_string(value).ok_or_else(|| bad("a string"))?)
            }
            "transport.kind" => {
                self.transport = parse_string(value).ok_or_else(|| bad("a string"))?
            }
            "transport.spool_dir" => {
                self.spool_dir = PathBuf::from(parse_string(value).ok_or_else(|| bad("a string"))?)
            }
            // Unknown keys are ignored so older builds can read newer files.
            _ => {}
        }
        Ok(())
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("# social-mail configuration\n\n");
        out.push_str("[profile]\n");
        out.push_str(&format!("name = {:?}\n", self.profile_name));
        out.push_str(&format!("address = {:?}\n", self.address));
        out.push_str(&format!("locale = {:?}\n\n", self.locale));
        out.push_str("[mail]\n");
        out.push_str(&format!(
            "move_protocol_messages = {}\n",
            self.move_protocol_messages
        ));
        out.push_str(&format!("protocol_folder = {:?}\n\n", self.protocol_folder));
        out.push_str("[ui]\n");
        out.push_str(&format!("color = {:?}\n", self.color));
        out.push_str(&format!(
            "density_following = {:?}\n",
            self.density_following
        ));
        out.push_str(&format!("density_lists = {:?}\n", self.density_lists));
        out.push_str(&format!("density_you = {:?}\n", self.density_you));
        out.push_str(&format!("density_all = {:?}\n", self.density_all));
        out.push_str(&format!("body_lines = {}\n\n", self.body_lines));
        out.push_str("[sync]\n");
        out.push_str(&format!(
            "auto_interval_minutes = {}\n\n",
            self.auto_sync_minutes
        ));
        out.push_str("[storage]\n");
        out.push_str(&format!(
            "data_dir = {:?}\n\n",
            self.data_dir.display().to_string()
        ));
        out.push_str("[transport]\n");
        out.push_str(&format!("kind = {:?}\n", self.transport));
        out.push_str(&format!(
            "spool_dir = {:?}\n",
            self.spool_dir.display().to_string()
        ));
        out
    }

    pub fn save(&self, path: &Path) -> Result<(), EngineError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, self.to_toml())?;
        Ok(())
    }

    pub fn density_for(&self, tab: &str) -> String {
        match tab {
            "following" => self.density_following.clone(),
            "lists" => self.density_lists.clone(),
            "you" => self.density_you.clone(),
            _ => self.density_all.clone(),
        }
    }

    pub fn set_density(&mut self, tab: &str, value: &str) {
        match tab {
            "following" => self.density_following = value.to_string(),
            "lists" => self.density_lists = value.to_string(),
            "you" => self.density_you = value.to_string(),
            _ => self.density_all = value.to_string(),
        }
    }

    pub fn as_map(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        map.insert("profile.name".into(), self.profile_name.clone());
        map.insert("profile.address".into(), self.address.clone());
        map.insert("profile.locale".into(), self.locale.clone());
        map.insert("ui.color".into(), self.color.clone());
        map.insert("ui.body_lines".into(), self.body_lines.to_string());
        map.insert(
            "mail.move_protocol_messages".into(),
            self.move_protocol_messages.to_string(),
        );
        map.insert("mail.protocol_folder".into(), self.protocol_folder.clone());
        map.insert(
            "sync.auto_interval_minutes".into(),
            self.auto_sync_minutes.to_string(),
        );
        map.insert("transport.kind".into(), self.transport.clone());
        map
    }
}

fn resolve(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return None;
    }
    let inner = &value[1..value.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                _ => return None,
            }
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_int(value: &str) -> Option<i64> {
    value.trim().parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config(text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sm-config-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("social-mail.toml");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn parses_the_documented_shape() {
        let path = temp_config(
            r#"
# a comment
[profile]
name = "Alice"
address = "alice@example.org"
locale = "fa"

[mail]
move_protocol_messages = false
protocol_folder = "SocialMail"

[ui]
color = "never"
body_lines = 4

[sync]
auto_interval_minutes = 5

[storage]
data_dir = "db"

[transport]
kind = "spool"
spool_dir = "/tmp/social-spool"
"#,
        );
        let config = Config::load(&path).unwrap();
        assert_eq!(config.profile_name, "Alice");
        assert_eq!(config.locale, "fa");
        assert!(!config.move_protocol_messages);
        assert_eq!(config.body_lines, 4);
        assert_eq!(config.auto_sync_minutes, 5);
        assert!(config.data_dir.ends_with("db"));
        assert_eq!(config.spool_dir, PathBuf::from("/tmp/social-spool"));
        assert_eq!(config.density_lists, "compact");
    }

    #[test]
    fn missing_file_yields_defaults() {
        let config = Config::load(Path::new("/nonexistent/social-mail.toml")).unwrap();
        assert_eq!(config.locale, "en");
        assert!(config.move_protocol_messages);
        assert_eq!(config.transport, "spool");
    }

    #[test]
    fn round_trips_through_save_and_load() {
        let path = temp_config("");
        let mut config = Config::load(&path).unwrap();
        config.profile_name = "Bob \"the builder\"".into();
        config.address = "bob@example.org".into();
        config.color = "always".into();
        config.body_lines = 9;
        config.save(&path).unwrap();
        let reloaded = Config::load(&path).unwrap();
        assert_eq!(reloaded.profile_name, config.profile_name);
        assert_eq!(reloaded.color, "always");
        assert_eq!(reloaded.body_lines, 9);
    }

    #[test]
    fn reports_malformed_lines_with_context() {
        let path = temp_config("[profile]\nname = Alice\n");
        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains("expects a string"), "{error}");

        let path = temp_config("[profile\nname = \"Alice\"\n");
        assert!(Config::load(&path).is_err());

        let path = temp_config("[ui]\nbody_lines = many\n");
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn comments_inside_strings_are_preserved() {
        let path = temp_config("[profile]\nname = \"A # B\"\n");
        let config = Config::load(&path).unwrap();
        assert_eq!(config.profile_name, "A # B");
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let path = temp_config("[future]\nthing = \"value\"\n");
        assert!(Config::load(&path).is_ok());
    }
}
