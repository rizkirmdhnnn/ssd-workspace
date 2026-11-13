//! macOS system access: volumes, Time Machine, Herd, processes using the SSD, and the
//! commands that actually change state (stopbackup, herd stop, eject).
//!
//! Every command uses an absolute path and is run directly through `std::process::Command`
//! (no shell), so nothing is ever interpolated into a shell string.
//!
//! Anything that runs repeatedly (mount checks, free space, Herd detection, mount
//! notifications) is done in-process through libc, without spawning a child process.

use serde::Serialize;
use std::collections::BTreeSet;
use std::ffi::CString;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

/// `/Volumes/<workspace volume>` (see `config.rs`).
pub fn workspace_path() -> &'static str {
    crate::config::get().workspace_path()
}

/// `/Volumes/<backup volume>`, if the disk has one (see `config.rs`).
pub fn backup_path() -> Option<&'static str> {
    crate::config::get().backup_path()
}

/// The configured volumes: the workspace volume, then the backup volume if any.
pub fn volume_paths() -> impl Iterator<Item = &'static str> {
    std::iter::once(workspace_path()).chain(backup_path())
}

pub const LSOF: &str = "/usr/sbin/lsof";
pub const DISKUTIL: &str = "/usr/sbin/diskutil";
pub const TMUTIL: &str = "/usr/bin/tmutil";
pub const OPEN: &str = "/usr/bin/open";

pub const HERD_BUNDLE_ID: &str = "de.beyondco.herd";

/// System processes that always touch a volume and do not block an eject.
pub const IGNORED_PROCESSES: &[&str] = &[
    "mds",
    "mds_stores",
    "mdworker",
    "mdworker_shared",
    "fseventsd",
    "lsof",
    "backupd",
    "revisiond",
];

pub fn herd_cli_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/Herd/bin/herd")
}

/// Result of running a command: exit code (-1 if it could not be run) and stdout+stderr combined.
pub struct CmdOutput {
    pub status: i32,
    pub output: String,
}

/// A `Command` with working directory `/`, so child processes never "use" the SSD
/// (and never block the eject) even when the app was launched from a folder on the SSD.
pub fn cmd(program: impl AsRef<Path>) -> Command {
    let mut c = Command::new(program.as_ref());
    c.current_dir("/");
    c
}

pub fn run(program: impl AsRef<Path>, args: &[&str]) -> CmdOutput {
    match cmd(program).args(args).output() {
        Ok(o) => {
            let mut output = String::from_utf8_lossy(&o.stdout).into_owned();
            output.push_str(&String::from_utf8_lossy(&o.stderr));
            CmdOutput {
                status: o.status.code().unwrap_or(-1),
                output,
            }
        }
        Err(e) => CmdOutput {
            status: -1,
            output: e.to_string(),
        },
    }
}

/// `true` when `path` is a real mount point: a directory on a different device than its
/// parent. A leftover (or not yet mounted) empty folder in `/Volumes` does not count, which
/// matters because mount notifications can arrive just before the folder is cleaned up.
pub fn is_mounted(path: &str) -> bool {
    let path = Path::new(path);
    let Some(parent) = path.parent() else {
        return false;
    };
    match (std::fs::metadata(path), std::fs::metadata(parent)) {
        (Ok(m), Ok(p)) => m.is_dir() && m.dev() != p.dev(),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Space {
    pub free: u64,
    pub total: u64,
}

/// Free and total space of a volume, as Finder shows it.
pub fn space_info(path: &str) -> Option<Space> {
    let c = CString::new(path).ok()?;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid C string and `st` is a large enough statfs buffer.
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let bsize = st.f_bsize as u64;
    Some(Space {
        free: st.f_bavail.saturating_mul(bsize),
        total: st.f_blocks.saturating_mul(bsize),
    })
}

/// "123 GB" (decimal, like Finder).
pub fn gb(bytes: u64) -> String {
    format!("{:.0} GB", bytes as f64 / 1_000_000_000.0)
}

pub fn time_machine_running() -> bool {
    run(TMUTIL, &["status"]).output.contains("Running = 1")
}

// ---------------------------------------------------------------------------------------
// Mount notifications

// <sys/mount.h>; not exported by the libc crate.
const VQ_MOUNT: u32 = 0x0008;
const VQ_UNMOUNT: u32 = 0x0010;

/// Call `on_change` whenever a filesystem is mounted or unmounted (kqueue `EVFILT_FS`).
/// The watcher thread sleeps inside the kernel until that happens, so it costs no CPU and
/// replaces polling `/Volumes`. It stops when `on_change` returns `false`.
///
/// Returns `false` when the watcher could not be set up; the caller should poll instead.
pub fn watch_mounts(on_change: impl Fn() -> bool + Send + 'static) -> bool {
    // SAFETY: plain kqueue calls on a descriptor we own, with valid kevent buffers.
    let kq = unsafe {
        let kq = libc::kqueue();
        if kq < 0 {
            return false;
        }
        let mut reg: libc::kevent = std::mem::zeroed();
        reg.filter = libc::EVFILT_FS;
        reg.flags = libc::EV_ADD | libc::EV_CLEAR;
        reg.fflags = VQ_MOUNT | VQ_UNMOUNT;
        if libc::kevent(kq, &reg, 1, std::ptr::null_mut(), 0, std::ptr::null()) < 0 {
            libc::close(kq);
            return false;
        }
        kq
    };
    thread::spawn(move || {
        loop {
            // SAFETY: as above; a null timeout blocks until an event arrives.
            let n = unsafe {
                let mut ev: libc::kevent = std::mem::zeroed();
                libc::kevent(kq, std::ptr::null(), 0, &mut ev, 1, std::ptr::null())
            };
            let interrupted =
                n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted;
            if (n < 0 && !interrupted) || (n > 0 && !on_change()) {
                break;
            }
        }
        // SAFETY: `kq` is owned by this thread and no longer used.
        unsafe { libc::close(kq) };
    });
    true
}

// ---------------------------------------------------------------------------------------
// Running from the SSD itself

/// When this app runs from an external volume (a dev build in `target/` on the SSD, or a
/// copy on a disk image), make a force-eject of that volume end the app instead of pinning
/// the CPU. Does nothing for the internal disk, e.g. `~/Applications`.
///
/// Why: a force-eject (Finder's Force Eject) takes the executable's code pages with it.
/// The next page fault raises SIGBUS, whose handler (installed by the Rust runtime for stack
/// overflow reports) lives in those same vanished pages, so delivering it faults again,
/// forever: two threads spinning at ~100% CPU each, inside the kernel. Pinning the pages
/// with `mlock` does not help; the kernel drops them anyway.
///
/// `_exit` lives in libSystem (always resident), so as the SIGBUS handler it ends the
/// process quietly without running any of our own code.
pub fn exit_on_vanished_code() {
    let Some(exe) = std::env::current_exe().ok() else {
        return;
    };
    let Ok(c) = CString::new(exe.as_os_str().as_encoded_bytes()) else {
        return;
    };
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid C string and `st` is a large enough statfs buffer.
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return;
    }
    // SAFETY: the kernel NUL-terminates f_mntonname.
    let mount = unsafe { std::ffi::CStr::from_ptr(st.f_mntonname.as_ptr()) };
    if !mount.to_bytes().starts_with(b"/Volumes/") {
        return;
    }
    // SAFETY: installs a plain (non-SA_SIGINFO) handler; `_exit(int)` has the matching
    // `void (*)(int)` calling convention and is async-signal-safe.
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = libc::_exit as *const () as usize;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGBUS, &sa, std::ptr::null_mut());
    }
}

// ---------------------------------------------------------------------------------------
// Processes

/// `true` if any running process has an executable path accepted by `pred`.
/// Walks the process table in-process (libproc), which also sees processes owned by root.
fn any_process_path(pred: impl Fn(&[u8]) -> bool) -> bool {
    // SAFETY: both buffers are valid for the sizes passed, and libproc never writes past them.
    unsafe {
        let count = libc::proc_listallpids(std::ptr::null_mut(), 0);
        if count <= 0 {
            return false;
        }
        // Headroom for processes started between the two calls.
        let mut pids = vec![0 as libc::pid_t; count as usize + 64];
        let bytes = (pids.len() * size_of::<libc::pid_t>()) as libc::c_int;
        let count = libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes);
        if count <= 0 {
            return false;
        }
        let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        pids.iter().take(count as usize).any(|&pid| {
            let len = libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32);
            len > 0 && pred(&path[..len as usize])
        })
    }
}

fn is_herd_service(path: &[u8]) -> bool {
    let name = path.rsplit(|b| *b == b'/').next().unwrap_or(path);
    name == b"nginx"
        || name == b"dnsmasq"
        || path
            .windows(b"Herd/bin/php".len())
            .any(|w| w == b"Herd/bin/php")
}

/// Herd services (nginx/dnsmasq/php-fpm) are serving sites.
pub fn herd_services_running() -> bool {
    any_process_path(is_herd_service)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct BusyProcess {
    pub name: String,
    pub pid: u32,
}

/// Parse `lsof -Fpc` output: a `p<pid>` line followed by `c<name>` (`f…` lines are ignored).
pub fn parse_lsof(output: &str, own_pid: u32, into: &mut BTreeSet<BusyProcess>) {
    let mut pid: Option<u32> = None;
    for line in output.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.trim().parse().ok();
        } else if let Some(name) = line.strip_prefix('c') {
            let Some(pid) = pid else { continue };
            if pid == own_pid || IGNORED_PROCESSES.contains(&name) {
                continue;
            }
            into.insert(BusyProcess {
                name: name.to_string(),
                pid,
            });
        }
    }
}

/// Processes (other than system processes and this app) that still have files open on the SSD.
/// Sorted by name (case-insensitive). Both volumes are checked with a single `lsof` run.
pub fn busy_processes() -> Vec<BusyProcess> {
    let volumes: Vec<&str> = volume_paths().filter(|v| is_mounted(v)).collect();
    if volumes.is_empty() {
        return Vec::new();
    }
    let mut set = BTreeSet::new();
    // Only stdout is parsed: lsof prints warnings about Time Machine snapshots to stderr.
    if let Ok(out) = cmd(LSOF).arg("-Fpc").args(&volumes).output() {
        parse_lsof(
            &String::from_utf8_lossy(&out.stdout),
            std::process::id(),
            &mut set,
        );
    }
    let mut v: Vec<_> = set.into_iter().collect();
    v.sort_by_cached_key(|p| (p.name.to_lowercase(), p.pid));
    v
}

pub fn open_in_finder(path: &str) {
    let _ = cmd(OPEN).arg(path).spawn();
}

// ---------------------------------------------------------------------------------------
// The Herd app (GUI) through NSRunningApplication: needs no Automation permission and
// NEVER launches the app (unlike `tell application … to quit`).

pub fn herd_app_running() -> bool {
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;
    let id = NSString::from_str(HERD_BUNDLE_ID);
    !NSRunningApplication::runningApplicationsWithBundleIdentifier(&id).is_empty()
}

/// Ask the Herd app to quit politely (the equivalent of Cmd+Q). Not a kill.
pub fn quit_herd_app() {
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;
    let id = NSString::from_str(HERD_BUNDLE_ID);
    for app in NSRunningApplication::runningApplicationsWithBundleIdentifier(&id).iter() {
        app.terminate();
    }
}

/// Run `herd stop`. If the Herd app was closed but got launched by the CLI, quit it again
/// (waiting a few seconds, because the app can show up slightly late).
pub fn herd_stop(app_was_open: bool) -> CmdOutput {
    let out = run(herd_cli_path(), &["stop"]);
    if !app_was_open {
        for _ in 0..10 {
            if herd_app_running() {
                quit_herd_app();
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsof_parsing_skips_system_and_self() {
        let out = "p100\nczsh\nfcwd\np200\ncmds_stores\nfcwd\np300\ncnode\nfcwd\nf12\np42\ncssd-workspace\nfcwd\n";
        let mut set = BTreeSet::new();
        parse_lsof(out, 42, &mut set);
        let names: Vec<_> = set.iter().map(|p| (p.name.as_str(), p.pid)).collect();
        assert_eq!(names, vec![("node", 300), ("zsh", 100)]);
    }

    #[test]
    fn gb_formatting() {
        assert_eq!(gb(999_500_000_000), "1000 GB");
        assert_eq!(gb(123_400_000_000), "123 GB");
    }

    #[test]
    fn herd_services_are_matched_by_executable_path() {
        let home = "/Users/x/Library/Application Support/Herd/bin";
        assert!(is_herd_service(format!("{home}/nginx").as_bytes()));
        assert!(is_herd_service(b"/opt/homebrew/sbin/dnsmasq"));
        assert!(is_herd_service(format!("{home}/php84").as_bytes()));
        assert!(!is_herd_service(format!("{home}/herd").as_bytes()));
        assert!(!is_herd_service(b"/usr/bin/php"));
        assert!(!is_herd_service(b"/usr/local/bin/nginx-ui"));
    }

    #[test]
    fn process_table_includes_root_and_own_process() {
        // launchd is owned by root: Herd's nginx and dnsmasq are too.
        assert!(any_process_path(|p| p == b"/sbin/launchd"));
        let me = std::env::current_exe().unwrap();
        let me = me.file_name().unwrap().as_encoded_bytes();
        assert!(any_process_path(|p| p.ends_with(me)));
        assert!(!any_process_path(|p| p == b"/no/such/binary"));
    }

    #[test]
    fn folders_and_missing_paths_are_not_mount_points() {
        assert!(is_mounted("/dev"));
        assert!(!is_mounted("/Volumes"));
        assert!(!is_mounted("/usr/bin"));
        assert!(!is_mounted("/Volumes/no-such-volume"));
    }
}
