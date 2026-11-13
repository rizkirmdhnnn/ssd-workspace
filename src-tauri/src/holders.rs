//! Who is holding the SSD, in terms the user can act on: apps (Terminal, an editor…) rather
//! than raw processes (`bash`, `node`, `2.1.323`). Also the APFS container breakdown for
//! the Detail window's storage bar.
//!
//! Every process with files open on the SSD (one `lsof` run) is walked up its parent chain
//! until a process inside an `.app` bundle is found; processes are grouped by that app.
//! Processes with no app ancestor (daemons, tools started by launchd) form a group of
//! their own, under their process name.

use crate::system::{self, IGNORED_PROCESSES};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Holder {
    /// "Terminal", "T3 Code", or the process name when there is no app.
    pub name: String,
    /// Path of the `.app` bundle (for its icon and for bringing it to the front).
    pub bundle: Option<String>,
    /// PNG data URL of the app icon (64×64).
    pub icon: Option<String>,
    /// The folder on the SSD the app is working in, e.g. "Workspace/Code/my-project".
    pub folder: Option<String>,
    /// How many processes of this app have files open on the SSD.
    pub processes: usize,
}

/// One process from `lsof -Fpcfn`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OpenFiles {
    pub pid: u32,
    pub name: String,
    /// Its working directory, when that is on the SSD.
    pub cwd: Option<String>,
    /// The first other open file on the SSD.
    pub first_file: Option<String>,
}

/// Parse `lsof -Fpcfn`: `p<pid>`, `c<command>`, then `f<fd>` / `n<path>` pairs per file.
pub fn parse_lsof_files(output: &str, own_pid: u32) -> Vec<OpenFiles> {
    let mut procs: Vec<OpenFiles> = Vec::new();
    let mut fd = String::new();
    for line in output.lines() {
        let (tag, value) = line.split_at(line.len().min(1));
        match tag {
            "p" => procs.push(OpenFiles {
                pid: value.trim().parse().unwrap_or(0),
                ..Default::default()
            }),
            "c" => {
                if let Some(p) = procs.last_mut() {
                    p.name = value.to_string();
                }
            }
            "f" => fd = value.to_string(),
            "n" => {
                if let Some(p) = procs.last_mut() {
                    if fd == "cwd" {
                        p.cwd = Some(value.to_string());
                    } else if p.first_file.is_none() {
                        p.first_file = Some(value.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    procs.retain(|p| {
        p.pid != 0 && p.pid != own_pid && !IGNORED_PROCESSES.contains(&p.name.as_str())
    });
    procs
}

fn lsof_open_files() -> Vec<OpenFiles> {
    let volumes: Vec<&str> = system::volume_paths()
        .filter(|v| system::is_mounted(v))
        .collect();
    if volumes.is_empty() {
        return Vec::new();
    }
    // Only stdout is parsed: lsof prints warnings about Time Machine snapshots to stderr.
    match system::cmd(system::LSOF)
        .arg("-Fpcfn")
        .args(&volumes)
        .output()
    {
        Ok(out) => parse_lsof_files(&String::from_utf8_lossy(&out.stdout), std::process::id()),
        Err(_) => Vec::new(),
    }
}

/// "/Volumes/Workspace/Code/x" → "Workspace/Code/x".
fn display_folder(path: &str) -> String {
    path.strip_prefix("/Volumes/").unwrap_or(path).to_string()
}

/// The folder a process is using: its working directory, else the folder of its first
/// open file.
fn folder_of(p: &OpenFiles) -> Option<String> {
    if let Some(cwd) = &p.cwd {
        return Some(display_folder(cwd));
    }
    let file = p.first_file.as_deref()?;
    let parent = std::path::Path::new(file).parent()?.to_str()?;
    (parent.len() > "/Volumes/".len()).then(|| display_folder(parent))
}

/// The outermost `.app` bundle in an executable path:
/// ".../T3 Code.app/Contents/Frameworks/Helper.app/Contents/MacOS/x" → ".../T3 Code.app".
pub fn app_bundle_of(path: &str) -> Option<&str> {
    path.find(".app/").map(|i| &path[..i + 4])
}

fn pid_path(pid: u32) -> Option<String> {
    let mut buf = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: `buf` is valid for `buf.len()` bytes and libproc never writes past it.
    let len = unsafe { libc::proc_pidpath(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32) };
    (len > 0).then(|| String::from_utf8_lossy(&buf[..len as usize]).into_owned())
}

fn parent_pid(pid: u32) -> Option<u32> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: `info` is a proc_bsdinfo, exactly what PROC_PIDTBSDINFO writes.
    let n = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    (n == size && info.pbi_ppid > 1).then_some(info.pbi_ppid)
}

/// The app a process belongs to: its own bundle, or the nearest ancestor's.
fn app_of(pid: u32) -> Option<String> {
    let mut pid = pid;
    for _ in 0..32 {
        if let Some(bundle) = pid_path(pid).as_deref().and_then(app_bundle_of) {
            return Some(bundle.to_string());
        }
        pid = parent_pid(pid)?;
    }
    None
}

fn app_name(bundle: &str) -> String {
    std::path::Path::new(bundle)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(bundle)
        .to_string()
}

/// Bundles returned by the last `list` call: the only ones `show_app` may open.
static KNOWN_BUNDLES: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Group the processes holding the SSD by app. Apps first (by name), then lone processes.
pub fn group(procs: &[OpenFiles], app_of: impl Fn(u32) -> Option<String>) -> Vec<Holder> {
    let mut groups: BTreeMap<(bool, String, String), Holder> = BTreeMap::new();
    let mut sorted: Vec<&OpenFiles> = procs.iter().collect();
    sorted.sort_by_key(|p| p.pid);
    for p in sorted {
        let bundle = app_of(p.pid);
        let name = bundle
            .as_deref()
            .map(app_name)
            .unwrap_or_else(|| p.name.clone());
        let key = (
            bundle.is_none(),
            name.to_lowercase(),
            bundle.clone().unwrap_or_default(),
        );
        let h = groups.entry(key).or_insert_with(|| Holder {
            name,
            bundle,
            icon: None,
            folder: None,
            processes: 0,
        });
        h.processes += 1;
        if h.folder.is_none() {
            h.folder = folder_of(p);
        }
    }
    groups.into_values().collect()
}

/// The last `list` result: shown at once when the Detail window opens, while a fresh check
/// runs. Cleared when the SSD is plugged in, unplugged or ejected.
static LAST: Mutex<Option<Vec<Holder>>> = Mutex::new(None);

/// Apps (and lone processes) holding the SSD, each app with its icon (rendered once per app,
/// then cached).
pub fn list() -> Vec<Holder> {
    let mut holders = group(&lsof_open_files(), app_of);
    *KNOWN_BUNDLES.lock().unwrap() =
        Some(holders.iter().filter_map(|h| h.bundle.clone()).collect());
    for h in &mut holders {
        h.icon = h.bundle.as_deref().and_then(icon_data_url);
    }
    *LAST.lock().unwrap() = Some(holders.clone());
    holders
}

/// The result of the last check, if it is still valid.
pub fn last() -> Option<Vec<Holder>> {
    LAST.lock().unwrap().clone()
}

/// Forget the last result (the SSD state changed).
pub fn invalidate() {
    *LAST.lock().unwrap() = None;
}

/// Bring an app from the last `list` to the front. Never quits or signals anything.
pub fn show_app(bundle: &str) -> Result<(), String> {
    let known = KNOWN_BUNDLES
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|k| k.contains(bundle));
    if !known {
        return Err("unknown app".into());
    }
    system::cmd(system::OPEN)
        .arg(bundle)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------
// App icons

static ICONS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

fn icon_data_url(bundle: &str) -> Option<String> {
    if let Some(cached) = ICONS.lock().unwrap().as_ref().and_then(|m| m.get(bundle)) {
        return cached.clone();
    }
    let url = render_icon(bundle).map(|png| format!("data:image/png;base64,{}", base64(&png)));
    ICONS
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(bundle.to_string(), url.clone());
    url
}

/// The Finder icon of `bundle` as a 64×64 PNG.
fn render_icon(bundle: &str) -> Option<Vec<u8>> {
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

    let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(bundle));
    let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(64.0, 64.0));
    // SAFETY: `rect` is a valid NSRect; no context or hints.
    let cg = unsafe { icon.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
    let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
    // SAFETY: an empty properties dictionary.
    let data = unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some(data.to_vec())
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// APFS container breakdown

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Storage {
    pub total: u64,
    pub free: u64,
    pub workspace: u64,
    pub backup: Option<u64>,
}

/// Container size and free space, and what each volume uses. In-process (`getattrlist`,
/// microseconds) instead of `diskutil info` (about 0.15 s per volume). `None` when the
/// Workspace volume is not mounted.
///
/// The Workspace volume has no quota, so its size and available space are the APFS
/// container's; a backup volume usually has a quota as its size, so only its use is taken.
pub fn storage() -> Option<Storage> {
    let ws = system::workspace_path();
    if !system::is_mounted(ws) {
        return None;
    }
    let (total, free, workspace) = vol_space(ws)?;
    let backup = system::backup_path()
        .filter(|b| system::is_mounted(b))
        .and_then(vol_space)
        .map(|(_, _, used)| used);
    Some(Storage {
        total,
        free,
        workspace,
        backup,
    })
}

/// (size, available, used) of a volume, in-process (getattrlist).
pub fn vol_space(path: &str) -> Option<(u64, u64, u64)> {
    #[repr(C, packed(4))]
    struct Buf {
        len: u32,
        size: i64,
        avail: i64,
        used: i64,
    }
    let c = std::ffi::CString::new(path).ok()?;
    let mut al: libc::attrlist = unsafe { std::mem::zeroed() };
    al.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
    al.volattr = libc::ATTR_VOL_INFO
        | libc::ATTR_VOL_SIZE
        | libc::ATTR_VOL_SPACEAVAIL
        | libc::ATTR_VOL_SPACEUSED;
    let mut buf: Buf = unsafe { std::mem::zeroed() };
    // SAFETY: valid attrlist and a buffer of the size passed.
    let r = unsafe {
        libc::getattrlist(
            c.as_ptr(),
            (&mut al as *mut libc::attrlist).cast(),
            (&mut buf as *mut Buf).cast(),
            size_of::<Buf>(),
            0,
        )
    };
    let (len, size, avail, used) = (buf.len, buf.size, buf.avail, buf.used);
    (r == 0 && len as usize >= size_of::<Buf>()).then_some((size as u64, avail as u64, used as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LSOF: &str = "p100\ncbash\nfcwd\nn/Volumes/Workspace/Code/app\nftxt\nn/Volumes/Workspace/bin/x\n\
p200\ncnode\nf12\nn/Volumes/Workspace/Code/app/index.js\n\
p300\ncmds\nfcwd\nn/Volumes/Workspace\n\
p42\nccself\nfcwd\nn/Volumes/Workspace\n\
p400\ncdaemon\nf3\nn/Volumes/Backup/log/a.log\n";

    #[test]
    fn parses_cwd_and_files_skipping_system_and_self() {
        let procs = parse_lsof_files(LSOF, 42);
        let pids: Vec<u32> = procs.iter().map(|p| p.pid).collect();
        assert_eq!(pids, [100, 200, 400]);
        assert_eq!(procs[0].cwd.as_deref(), Some("/Volumes/Workspace/Code/app"));
        assert_eq!(
            procs[0].first_file.as_deref(),
            Some("/Volumes/Workspace/bin/x")
        );
        assert_eq!(procs[1].cwd, None);
    }

    #[test]
    fn groups_by_app_then_lone_processes() {
        let procs = parse_lsof_files(LSOF, 42);
        let holders = group(&procs, |pid| match pid {
            100 | 200 => Some("/System/Applications/Utilities/Terminal.app".into()),
            _ => None,
        });
        assert_eq!(holders.len(), 2);
        assert_eq!(holders[0].name, "Terminal");
        assert_eq!(holders[0].processes, 2);
        assert_eq!(holders[0].folder.as_deref(), Some("Workspace/Code/app"));
        assert_eq!(holders[1].name, "daemon");
        assert_eq!(holders[1].bundle, None);
        assert_eq!(holders[1].folder.as_deref(), Some("Backup/log"));
    }

    #[test]
    fn outermost_app_bundle() {
        assert_eq!(
            app_bundle_of("/Applications/T3 Code.app/Contents/Frameworks/H.app/Contents/MacOS/h"),
            Some("/Applications/T3 Code.app")
        );
        assert_eq!(app_bundle_of("/usr/bin/bash"), None);
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    #[ignore = "reads the real system"]
    fn real_holders_and_storage() {
        for h in list() {
            println!(
                "{} ({} procs) {:?} {:?} icon={}",
                h.name,
                h.processes,
                h.bundle,
                h.folder,
                h.icon.as_ref().map_or(0, |i| i.len())
            );
        }
        println!("{:?}", storage());
    }
}
