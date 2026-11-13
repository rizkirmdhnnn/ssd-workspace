//! Which volumes this app manages.
//!
//! Read once at startup from `~/Library/Application Support/<bundle identifier>/config.json`:
//!
//! ```json
//! { "workspaceVolume": "Workspace", "backupVolume": "Backup" }
//! ```
//!
//! - `workspaceVolume`: the volume you work on (`/Volumes/<name>`). Ejecting it ejects the
//!   whole disk, including every other volume on it.
//! - `backupVolume`: an optional second volume on the same disk, usually the Time Machine
//!   destination. Use `null` if there is none.
//!
//! The file is optional; without it (or when it cannot be read) the defaults below apply.
//! Changes take effect after restarting the app.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const DEFAULT_WORKSPACE: &str = "Workspace";
pub const DEFAULT_BACKUP: &str = "Backup";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Volume name, e.g. "Workspace".
    pub workspace: String,
    /// Volume name, e.g. "Backup"; `None` when the disk has no backup volume.
    pub backup: Option<String>,
    workspace_path: String,
    backup_path: Option<String>,
}

impl Config {
    fn new(workspace: String, backup: Option<String>) -> Self {
        Config {
            workspace_path: format!("/Volumes/{workspace}"),
            backup_path: backup.as_ref().map(|b| format!("/Volumes/{b}")),
            workspace,
            backup,
        }
    }

    pub fn workspace_path(&self) -> &str {
        &self.workspace_path
    }

    pub fn backup_path(&self) -> Option<&str> {
        self.backup_path.as_deref()
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::new(DEFAULT_WORKSPACE.into(), Some(DEFAULT_BACKUP.into()))
    }
}

/// The JSON file as written by the user. A missing key keeps its default; an explicit
/// `"backupVolume": null` means "no backup volume".
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct File {
    workspace_volume: Option<String>,
    #[serde(default, deserialize_with = "present")]
    backup_volume: Option<Option<String>>,
}

/// Wraps a present value (including `null`) in `Some`, so a missing key (`None`) can be told
/// apart from an explicit `null` (`Some(None)`).
fn present<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(d).map(Some)
}

/// A volume name must be a single path component under `/Volumes`.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

/// Parse the config file contents. Invalid names are rejected with a message.
pub fn parse(json: &str) -> Result<Config, String> {
    let file: File = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let workspace = file
        .workspace_volume
        .unwrap_or_else(|| DEFAULT_WORKSPACE.into());
    let backup = match file.backup_volume {
        None => Some(DEFAULT_BACKUP.into()),
        Some(b) => b,
    };
    if !valid_name(&workspace) {
        return Err(format!("invalid workspaceVolume: {workspace:?}"));
    }
    if let Some(b) = &backup
        && (!valid_name(b) || *b == workspace)
    {
        return Err(format!("invalid backupVolume: {b:?}"));
    }
    Ok(Config::new(workspace, backup))
}

pub fn file_path(identifier: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        Path::new(&home)
            .join("Library/Application Support")
            .join(identifier)
            .join("config.json"),
    )
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Load the config file for this bundle identifier. Call once, before anything reads the
/// config; later calls are ignored. Problems are logged and the defaults are used.
pub fn load(identifier: &str) {
    let config = match file_path(identifier).map(|p| (std::fs::read_to_string(&p), p)) {
        Some((Ok(json), path)) => parse(&json).unwrap_or_else(|e| {
            eprintln!(
                "[ssd-workspace] ignoring {}: {e}; using the defaults",
                path.display()
            );
            Config::default()
        }),
        _ => Config::default(),
    };
    let _ = CONFIG.set(config);
}

/// The active config (the defaults if `load` was never called, e.g. in tests).
pub fn get() -> &'static Config {
    CONFIG.get_or_init(Config::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_keys_are_missing() {
        let c = parse("{}").unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.workspace_path(), "/Volumes/Workspace");
        assert_eq!(c.backup_path(), Some("/Volumes/Backup"));
    }

    #[test]
    fn custom_names_and_no_backup() {
        let c = parse(r#"{"workspaceVolume": "Projects", "backupVolume": null}"#).unwrap();
        assert_eq!(c.workspace_path(), "/Volumes/Projects");
        assert_eq!(c.backup, None);
        assert_eq!(c.backup_path(), None);

        let c = parse(r#"{"backupVolume": "TM"}"#).unwrap();
        assert_eq!(c.workspace, "Workspace");
        assert_eq!(c.backup_path(), Some("/Volumes/TM"));
    }

    #[test]
    fn rejects_paths_and_bad_names() {
        for json in [
            r#"{"workspaceVolume": ""}"#,
            r#"{"workspaceVolume": "../etc"}"#,
            r#"{"workspaceVolume": "a/b"}"#,
            r#"{"workspaceVolume": ".."}"#,
            r#"{"backupVolume": "Workspace"}"#,
            r#"{"workspaceVolume": "W", "extra": 1}"#,
            "not json",
        ] {
            assert!(parse(json).is_err(), "{json} should be rejected");
        }
    }
}
