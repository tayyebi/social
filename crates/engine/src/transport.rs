//! File-based transport ("local mode"): one spool directory per fingerprint.
//!
//! This is the transport that v0.1 ships with. It is honest about what it is: a
//! shared directory that any file-sync tool (or two profiles on one machine) can
//! move. SMTP/IMAP are the next milestone; see `TODO.md`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::EngineError;

pub struct Incoming {
    pub path: PathBuf,
    pub body: String,
}

pub struct SpoolTransport {
    root: PathBuf,
    me: String,
}

impl SpoolTransport {
    pub fn new(root: impl Into<PathBuf>, me: impl Into<String>) -> Result<Self, EngineError> {
        let root = root.into();
        let me = me.into();
        if me.is_empty() || !me.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(EngineError::Transport(
                "spool recipient must be a hex fingerprint".into(),
            ));
        }
        fs::create_dir_all(root.join(&me))?;
        Ok(SpoolTransport { root, me })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Write one carrier into the recipient's spool directory.
    pub fn send(&self, recipient: &str, body: &str) -> Result<String, EngineError> {
        if recipient.is_empty() || !recipient.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(EngineError::Transport(format!(
                "cannot spool a message for {recipient:?}"
            )));
        }
        let dir = self.root.join(recipient);
        fs::create_dir_all(&dir)?;
        let stamp = chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default();
        let name = format!("{stamp:020}-{}.json", std::process::id());
        let temp = dir.join(format!(".{name}.tmp"));
        fs::write(&temp, body)?;
        fs::rename(&temp, dir.join(&name))?;
        Ok(name)
    }

    pub fn receive(&self) -> Result<Vec<Incoming>, EngineError> {
        let dir = self.root.join(&self.me);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut paths: Vec<PathBuf> = fs::read_dir(&dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .map(|ext| ext == "json")
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();
        let mut out = Vec::new();
        for path in paths {
            let body = fs::read_to_string(&path)?;
            out.push(Incoming { path, body });
        }
        Ok(out)
    }

    pub fn acknowledge(&self, path: &Path) -> Result<(), EngineError> {
        let done = path.with_extension("json.done");
        fs::rename(path, done)?;
        Ok(())
    }

    pub fn quarantine(&self, path: &Path) -> Result<(), EngineError> {
        let rejected = path.with_extension("json.rejected");
        fs::rename(path, rejected)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sm-spool-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn spools_and_receives_a_message() {
        let root = temp_root();
        let alice = SpoolTransport::new(&root, "AA".repeat(20)).unwrap();
        let bob = SpoolTransport::new(&root, "BB".repeat(20)).unwrap();
        alice.send(&("BB".repeat(20)), "{\"hello\":true}").unwrap();
        let incoming = bob.receive().unwrap();
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].body, "{\"hello\":true}");
        bob.acknowledge(&incoming[0].path).unwrap();
        assert!(bob.receive().unwrap().is_empty());
    }

    #[test]
    fn rejects_non_hex_recipients() {
        let root = temp_root();
        let alice = SpoolTransport::new(&root, "AA".repeat(20)).unwrap();
        assert!(alice.send("bob@example.org", "{}").is_err());
        assert!(SpoolTransport::new(&root, "../evil").is_err());
    }

    #[test]
    fn quarantine_moves_the_file_aside() {
        let root = temp_root();
        let alice = SpoolTransport::new(&root, "AA".repeat(20)).unwrap();
        let bob = SpoolTransport::new(&root, "BB".repeat(20)).unwrap();
        alice.send(&("BB".repeat(20)), "garbage").unwrap();
        let incoming = bob.receive().unwrap();
        bob.quarantine(&incoming[0].path).unwrap();
        assert!(bob.receive().unwrap().is_empty());
        assert!(incoming[0].path.with_extension("json.rejected").exists());
    }
}
