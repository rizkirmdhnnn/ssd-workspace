//! Time Machine: backup status (phase and progress), the latest backup, and the actions
//! that mirror the built-in macOS Time Machine menu.
//!
//! `tmutil latestbackup` needs Full Disk Access, so the date of the latest backup is read
//! from the Time Machine preferences through `defaults export` (no special permission).
//!
//! Status and preferences are read separately: the status changes while a backup runs, the
//! preferences only when one finishes, so callers can cache [`Prefs`] between backups.

use crate::system::{CmdOutput, OPEN, TMUTIL, cmd, run};
use plist::{Dictionary, Value};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULTS: &str = "/usr/bin/defaults";
const PREFS_DOMAIN: &str = "/Library/Preferences/com.apple.TimeMachine";
/// Bundle ID of the "Time Machine" app (the backup browser).
pub const BROWSER_BUNDLE_ID: &str = "com.apple.backup.launcher";
pub const SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.Time-Machine-Settings.extension";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Preparing,
    Copying,
    Finishing,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Progress {
    pub phase: Phase,
    /// 0.0–1.0 when known.
    pub fraction: Option<f64>,
    pub bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    /// Time Machine's own estimate, in seconds (copying only).
    pub time_remaining: Option<u64>,
}

/// What `tmutil status` reports: whether a backup is running, and how far along it is.
pub type Status = (bool, Option<Progress>);

/// The part of the Time Machine preferences this app shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prefs {
    /// A backup disk is configured.
    pub configured: bool,
    /// Name of the backup disk, e.g. "Backup".
    pub destination: Option<String>,
    /// Time of the latest backup (Unix seconds).
    pub latest: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TmInfo {
    /// A backup disk is configured.
    pub configured: bool,
    /// Name of the backup disk, e.g. "Backup".
    pub destination: Option<String>,
    /// Time of the latest backup (Unix seconds).
    pub latest: Option<i64>,
    /// "Today, 18:09" / "Yesterday, 20:00" / "12 Nov, 20:00".
    pub latest_text: Option<String>,
    pub running: bool,
    pub progress: Option<Progress>,
    /// Ready-to-show status sentence, e.g. "Backing up: 1.2 GB of 3.4 GB".
    pub status_text: String,
    /// "About 3 minutes remaining", while copying and Time Machine has an estimate.
    pub remaining_text: Option<String>,
}

// ---------------------------------------------------------------------------------------
// Parsing

fn real(d: &Dictionary, key: &str) -> Option<f64> {
    match d.get(key)? {
        Value::Real(r) => Some(*r),
        Value::Integer(i) => i.as_signed().map(|v| v as f64),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn uint(d: &Dictionary, key: &str) -> Option<u64> {
    real(d, key).filter(|v| *v >= 0.0).map(|v| v as u64)
}

/// Parse `tmutil status -X`.
pub fn parse_status(xml: &[u8]) -> Status {
    let Ok(Value::Dictionary(d)) = Value::from_reader_xml(xml) else {
        return (false, None);
    };
    let running = match d.get("Running") {
        Some(Value::Boolean(b)) => *b,
        Some(Value::Integer(i)) => i.as_signed() == Some(1),
        _ => false,
    };
    if !running {
        return (false, None);
    }
    let phase = match d.get("BackupPhase").and_then(Value::as_string) {
        Some("Copying") => Phase::Copying,
        Some("ThinningPostBackup" | "Finishing" | "DeletingOldBackups") => Phase::Finishing,
        _ => Phase::Preparing,
    };
    let prog = d.get("Progress").and_then(Value::as_dictionary);
    let fraction = prog
        .and_then(|p| real(p, "Percent"))
        .or_else(|| real(&d, "Percent"))
        .filter(|f| (0.0..=1.0).contains(f));
    (
        true,
        Some(Progress {
            phase,
            fraction,
            bytes: prog.and_then(|p| uint(p, "bytes")),
            total_bytes: prog.and_then(|p| uint(p, "totalBytes")),
            time_remaining: prog
                .and_then(|p| uint(p, "TimeRemaining"))
                .or_else(|| uint(&d, "TimeRemaining")),
        }),
    )
}

/// Parse `defaults export /Library/Preferences/com.apple.TimeMachine -`.
pub fn parse_prefs(xml: &[u8]) -> Prefs {
    let Ok(Value::Dictionary(d)) = Value::from_reader_xml(xml) else {
        return Prefs::default();
    };
    let dests: Vec<&Dictionary> = d
        .get("Destinations")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_dictionary).collect())
        .unwrap_or_default();
    let last_id = d.get("LastDestinationID").and_then(Value::as_string);
    let dest = dests
        .iter()
        .find(|x| x.get("DestinationID").and_then(Value::as_string) == last_id && last_id.is_some())
        .or(dests.first())
        .copied();
    let Some(dest) = dest else {
        return Prefs::default();
    };
    let destination = dest
        .get("LastKnownVolumeName")
        .and_then(Value::as_string)
        .map(str::to_string);
    let latest = dest
        .get("SnapshotDates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_date)
        .filter_map(|d| SystemTime::from(d).duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .max();
    Prefs {
        configured: true,
        destination,
        latest,
    }
}

// ---------------------------------------------------------------------------------------
// Formatting

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

struct LocalTime {
    year: i32,
    yday: i32,
    month: usize,
    day: i32,
    hour: i32,
    min: i32,
}

fn local(t: i64) -> Option<LocalTime> {
    let tt = t as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are valid for the duration of the call; localtime_r is thread-safe.
    if unsafe { libc::localtime_r(&tt, &mut tm) }.is_null() {
        return None;
    }
    Some(LocalTime {
        year: tm.tm_year + 1900,
        yday: tm.tm_yday,
        month: tm.tm_mon as usize,
        day: tm.tm_mday,
        hour: tm.tm_hour,
        min: tm.tm_min,
    })
}

/// "Today, 18:09", "Yesterday, 20:00", "12 Nov, 20:00", "12 Nov 2025, 20:00".
pub fn format_when(t: i64, now: i64) -> String {
    let (Some(a), Some(n)) = (local(t), local(now)) else {
        return String::new();
    };
    let clock = format!("{:02}:{:02}", a.hour, a.min);
    let yesterday = local(now - 86_400);
    if a.year == n.year && a.yday == n.yday {
        format!("Today, {clock}")
    } else if yesterday.is_some_and(|y| y.year == a.year && y.yday == a.yday) {
        format!("Yesterday, {clock}")
    } else if a.year == n.year {
        format!("{} {}, {clock}", a.day, MONTHS[a.month.min(11)])
    } else {
        format!("{} {} {}, {clock}", a.day, MONTHS[a.month.min(11)], a.year)
    }
}

/// Finder-style (decimal) size: "1.2 GB", "850 MB".
pub fn format_bytes(b: u64) -> String {
    let units = [("TB", 1e12), ("GB", 1e9), ("MB", 1e6), ("KB", 1e3)];
    for (u, s) in units {
        let v = b as f64 / s;
        if v >= 1.0 {
            let txt = if v >= 100.0 {
                format!("{v:.0}")
            } else {
                format!("{v:.1}")
            };
            return format!("{txt} {u}");
        }
    }
    if b == 0 {
        "0 KB".into()
    } else {
        format!("{b} bytes")
    }
}

/// Time Machine's wording: "About 3 minutes remaining".
pub fn format_remaining(secs: u64) -> String {
    let n = |v: u64, unit: &str| {
        format!(
            "About {v} {unit}{} remaining",
            if v == 1 { "" } else { "s" }
        )
    };
    match secs {
        0..60 => "Less than a minute remaining".into(),
        60..3600 => n(secs.div_ceil(60), "minute"),
        _ => n((secs + 1800) / 3600, "hour"),
    }
}

pub fn remaining_text(info: &TmInfo) -> Option<String> {
    let p = info.progress.as_ref()?;
    (p.phase == Phase::Copying)
        .then_some(p.time_remaining?)
        .map(format_remaining)
}

pub fn status_text(info: &TmInfo) -> String {
    if let Some(p) = &info.progress {
        return match p.phase {
            Phase::Preparing => "Preparing backup…".into(),
            Phase::Finishing => "Finishing backup…".into(),
            Phase::Copying => match (p.bytes, p.total_bytes) {
                (Some(b), Some(t)) if t > 0 => {
                    format!("Backing up: {} of {}", format_bytes(b), format_bytes(t))
                }
                _ => match p.fraction {
                    Some(f) => format!("Backing up: {:.0}%", f * 100.0),
                    None => "Backing up…".into(),
                },
            },
        };
    }
    if !info.configured {
        return "Time Machine is not set up".into();
    }
    match &info.latest_text {
        Some(t) => format!("Latest backup: {t}"),
        None => "No backups yet".into(),
    }
}

// ---------------------------------------------------------------------------------------
// System

/// Whether a backup is running and its progress (one `tmutil status -X` run).
pub fn read_status() -> Status {
    cmd(TMUTIL)
        .args(["status", "-X"])
        .output()
        .map(|o| parse_status(&o.stdout))
        .unwrap_or((false, None))
}

/// Backup disk and latest backup (one `defaults export` run).
pub fn read_prefs() -> Prefs {
    cmd(DEFAULTS)
        .args(["export", PREFS_DOMAIN, "-"])
        .output()
        .map(|o| parse_prefs(&o.stdout))
        .unwrap_or_default()
}

/// Combine preferences and status into what the UI shows. Runs no command, so the relative
/// date ("Today" / "Yesterday") can be refreshed from cached values.
pub fn build_info(prefs: &Prefs, (running, progress): Status) -> TmInfo {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut info = TmInfo {
        configured: prefs.configured,
        destination: prefs.destination.clone(),
        latest: prefs.latest,
        latest_text: prefs.latest.map(|t| format_when(t, now)),
        running,
        progress,
        status_text: String::new(),
        remaining_text: None,
    };
    info.status_text = status_text(&info);
    info.remaining_text = remaining_text(&info);
    info
}

/// Everything, read fresh (two commands).
pub fn info() -> TmInfo {
    build_info(&read_prefs(), read_status())
}

/// "Back Up Now" (same as the built-in menu).
pub fn start_backup() -> CmdOutput {
    run(TMUTIL, &["startbackup"])
}

/// "Skip This Backup".
pub fn skip_backup() -> CmdOutput {
    run(TMUTIL, &["stopbackup"])
}

/// "Browse Time Machine Backups".
pub fn browse() {
    let _ = cmd(OPEN).args(["-b", BROWSER_BUNDLE_ID]).spawn();
}

/// "Open Time Machine Settings…".
pub fn open_settings() {
    let _ = cmd(OPEN).arg(SETTINGS_URL).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>ClientID</key><string>com.apple.backupd</string>
<key>Percent</key><real>-1</real>
<key>Running</key><false/>
</dict></plist>"#;

    const COPYING: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>BackupPhase</key><string>Copying</string>
<key>Percent</key><real>0.42</real>
<key>Progress</key><dict>
  <key>Percent</key><real>0.42</real>
  <key>bytes</key><integer>1234000000</integer>
  <key>totalBytes</key><integer>3400000000</integer>
  <key>TimeRemaining</key><real>150</real>
</dict>
<key>Running</key><true/>
</dict></plist>"#;

    const PREFS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>Destinations</key><array>
  <dict>
    <key>DestinationID</key><string>OTHER</string>
    <key>LastKnownVolumeName</key><string>Old</string>
    <key>SnapshotDates</key><array><date>2030-01-01T00:00:00Z</date></array>
  </dict>
  <dict>
    <key>DestinationID</key><string>F6CA</string>
    <key>LastKnownVolumeName</key><string>Backup</string>
    <key>SnapshotDates</key><array>
      <date>2026-11-13T10:08:33Z</date>
      <date>2026-11-13T11:09:58Z</date>
    </array>
  </dict>
</array>
<key>LastDestinationID</key><string>F6CA</string>
</dict></plist>"#;

    #[test]
    fn remaining_wording() {
        assert_eq!(format_remaining(20), "Less than a minute remaining");
        assert_eq!(format_remaining(60), "About 1 minute remaining");
        assert_eq!(format_remaining(150), "About 3 minutes remaining");
        assert_eq!(format_remaining(5400), "About 2 hours remaining");
        let (running, progress) = parse_status(COPYING.as_bytes());
        let info = build_info(&Prefs::default(), (running, progress));
        assert_eq!(
            info.remaining_text.as_deref(),
            Some("About 3 minutes remaining")
        );
    }

    #[test]
    fn idle_status() {
        assert_eq!(parse_status(IDLE.as_bytes()), (false, None));
    }

    #[test]
    fn copying_status_and_text() {
        let (running, progress) = parse_status(COPYING.as_bytes());
        assert!(running);
        let p = progress.unwrap();
        assert_eq!(p.phase, Phase::Copying);
        assert_eq!(p.fraction, Some(0.42));
        let info = TmInfo {
            configured: true,
            running,
            progress: Some(p),
            ..Default::default()
        };
        assert_eq!(status_text(&info), "Backing up: 1.2 GB of 3.4 GB");
    }

    #[test]
    fn prefs_use_last_destination_and_latest_date() {
        let prefs = parse_prefs(PREFS.as_bytes());
        assert!(prefs.configured);
        assert_eq!(prefs.destination.as_deref(), Some("Backup"));
        assert_eq!(prefs.latest, Some(1_794_568_198)); // 2026-11-13T11:09:58Z
    }

    #[test]
    fn not_configured() {
        let prefs = parse_prefs(b"<plist version=\"1.0\"><dict/></plist>");
        assert_eq!(prefs, Prefs::default());
        let info = build_info(&prefs, (false, None));
        assert_eq!(info.status_text, "Time Machine is not set up");
    }

    #[test]
    fn bytes_formatting() {
        assert_eq!(format_bytes(850_000_000), "850 MB");
        assert_eq!(format_bytes(1_234_000_000), "1.2 GB");
        assert_eq!(format_bytes(12_000), "12.0 KB");
        assert_eq!(format_bytes(0), "0 KB");
    }

    #[test]
    fn relative_dates() {
        let now = 1_794_568_198 + 3600;
        assert!(format_when(1_794_568_198, now).starts_with("Today, "));
        assert!(format_when(1_794_568_198 - 86_400, now).starts_with("Yesterday, "));
        assert!(format_when(1_794_568_198 - 86_400 * 5, now).contains("Nov"));
    }
}

/// `cargo test real_tm_info -- --ignored --nocapture`: read the real Time Machine status (read-only).
#[cfg(test)]
#[test]
#[ignore = "reads the real system"]
fn real_tm_info() {
    let i = info();
    println!("{i:#?}");
    assert!(!i.status_text.is_empty());
}
