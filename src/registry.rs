use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct MountEntry {
    pub pid: u32,
    pub source: String,
    pub mountpoint: String,
    pub started_at: u64,
}

impl MountEntry {
    fn uptime_secs(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.started_at)
    }

    pub fn uptime_str(&self) -> String {
        let secs = self.uptime_secs();
        if secs < 60 {
            format!("{secs}s")
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }
    }

    fn serialize(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}\n",
            self.pid,
            escape(&self.source),
            escape(&self.mountpoint),
            self.started_at
        )
    }

    fn deserialize(s: &str) -> Option<Self> {
        let mut lines = s.lines();
        Some(MountEntry {
            pid: lines.next()?.parse().ok()?,
            source: unescape(lines.next()?),
            mountpoint: unescape(lines.next()?),
            started_at: lines.next()?.parse().ok()?,
        })
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') | None => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub struct MountRegistry {
    dir: PathBuf,
}

impl MountRegistry {
    pub fn new() -> Result<Self> {
        let dir = Self::registry_dir()?;
        std::fs::create_dir_all(&dir).context("failed to create registry directory")?;
        Ok(Self { dir })
    }

    fn registry_dir() -> Result<PathBuf> {
        if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
            Ok(Path::new(&runtime).join("clean-mount/mounts"))
        } else if let Ok(home) = std::env::var("HOME") {
            Ok(Path::new(&home).join(".local/share/clean-mount/mounts"))
        } else {
            anyhow::bail!("neither XDG_RUNTIME_DIR nor HOME is set");
        }
    }

    fn pid_path(&self, pid: u32) -> PathBuf {
        self.dir.join(format!("{pid}.mount"))
    }

    pub fn register(&self, source: &Path, mountpoint: &Path, pid: u32) -> Result<MountEntry> {
        let entry = MountEntry {
            pid,
            source: source.to_string_lossy().into_owned(),
            mountpoint: mountpoint.to_string_lossy().into_owned(),
            started_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        let path = self.pid_path(pid);
        std::fs::write(&path, entry.serialize()).context("failed to write registry entry")?;
        Ok(entry)
    }

    pub fn unregister(&self, pid: u32) -> Result<()> {
        let path = self.pid_path(pid);
        if path.exists() {
            std::fs::remove_file(&path).context("failed to remove registry entry")?;
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<MountEntry>> {
        let mut entries = Vec::new();
        if !self.dir.exists() {
            return Ok(entries);
        }
        for entry in std::fs::read_dir(&self.dir).context("failed to read registry directory")? {
            let entry = entry.context("failed to read registry entry")?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.ends_with(".mount") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Some(me) = MountEntry::deserialize(&content) {
                        if process_exists(me.pid) {
                            entries.push(me);
                        }
                    }
                }
            }
        }
        entries.sort_by_key(|e| e.pid);
        Ok(entries)
    }

    pub fn lookup_by_pid(&self, pid: u32) -> Result<Option<MountEntry>> {
        let path = self.pid_path(pid);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(MountEntry::deserialize(&content))
    }

    pub fn lookup_by_mountpoint(&self, mountpoint: &Path) -> Result<Vec<MountEntry>> {
        let mp_str = mountpoint.to_string_lossy();
        let mut found = Vec::new();
        for entry in self.list()? {
            if entry.mountpoint == mp_str.as_ref() {
                found.push(entry);
            }
        }
        Ok(found)
    }
}

fn process_exists(pid: u32) -> bool {
    if unsafe { libc::kill(pid as i32, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_round_trips_paths_with_newlines_and_backslashes() {
        let entry = MountEntry {
            pid: 1234,
            source: "/tmp/src\nwith\\quirk".to_string(),
            mountpoint: "/mnt/clean\nmount".to_string(),
            started_at: 42,
        };

        let restored = MountEntry::deserialize(&entry.serialize()).unwrap();

        assert_eq!(restored.pid, entry.pid);
        assert_eq!(restored.source, entry.source);
        assert_eq!(restored.mountpoint, entry.mountpoint);
        assert_eq!(restored.started_at, entry.started_at);
    }

    #[test]
    fn serialize_round_trips_plain_paths() {
        let entry = MountEntry {
            pid: 7,
            source: "/tmp/src".to_string(),
            mountpoint: "/mnt/view".to_string(),
            started_at: 0,
        };

        let restored = MountEntry::deserialize(&entry.serialize()).unwrap();

        assert_eq!(restored.pid, entry.pid);
        assert_eq!(restored.source, entry.source);
        assert_eq!(restored.mountpoint, entry.mountpoint);
        assert_eq!(restored.started_at, entry.started_at);
    }

    #[test]
    fn deserialize_returns_none_for_missing_fields() {
        assert!(MountEntry::deserialize("1234\nonly-one-line\n").is_none());
    }

    #[test]
    fn escape_handles_backslashes_and_newlines() {
        assert_eq!(escape("a\\b\nc"), "a\\\\b\\nc");
        assert_eq!(unescape("a\\\\b\\nc"), "a\\b\nc");
    }
}
