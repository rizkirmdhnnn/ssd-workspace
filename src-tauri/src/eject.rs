//! The SSD eject flow, separated from Tauri by the [`Env`] trait so it can be tested
//! without touching the real SSD.
//!
//! Safety rules enforced here (and tested below):
//! - Never eject without confirmation.
//! - Herd is only touched when its services are actually running.
//! - Dry-run mode never calls `stop_backup`, `herd_stop`, or `eject`.
//! - No user process is ever killed: we only list them.

use crate::config;
use crate::system::{self, BusyProcess, CmdOutput, DISKUTIL, TMUTIL};

/// Everything the eject flow needs from the outside world.
pub trait Env {
    // --- State checks (read-only, also used during a dry run) ---
    fn workspace_mounted(&self) -> bool;
    fn time_machine_running(&self) -> bool;
    fn herd_services_running(&self) -> bool;
    fn herd_cli_available(&self) -> bool;
    fn herd_app_running(&self) -> bool;
    fn busy_processes(&self) -> Vec<BusyProcess>;
    fn herd_cli_display(&self) -> String;

    // --- Dialog ---
    /// `true` when the `ok` button is chosen.
    fn confirm(&self, title: &str, text: &str, ok: &str, warning: bool) -> bool;
    fn alert(&self, title: &str, text: &str, warning: bool);

    // --- Actions that change state (NOT called during a dry run) ---
    /// `tmutil stopbackup`, then wait until the backup has really stopped (up to ~30 seconds).
    fn stop_backup(&self);
    fn herd_stop(&self, app_was_open: bool);
    fn eject(&self) -> CmdOutput;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    NotMounted,
    Cancelled,
    /// Cancelled in the "SSD is still in use" dialog.
    CancelledWhileBusy,
    Ejected,
    EjectFailed,
    /// Dry run: the list of commands that *would* be run.
    DryRun(Vec<String>),
}

pub const MAX_LISTED: usize = 12;

pub fn busy_list_text(procs: &[BusyProcess]) -> String {
    let mut s = procs
        .iter()
        .take(MAX_LISTED)
        .map(|p| format!("• {}  (PID {})", p.name, p.pid))
        .collect::<Vec<_>>()
        .join("\n");
    if procs.len() > MAX_LISTED {
        s.push_str(&format!("\n…and {} more", procs.len() - MAX_LISTED));
    }
    s
}

pub fn eject_flow(env: &dyn Env, dry_run: bool) -> Outcome {
    // 1. Not mounted → just say so.
    if !env.workspace_mounted() {
        env.alert(
            "SSD Workspace is not mounted",
            "There is nothing to eject.",
            false,
        );
        return Outcome::NotMounted;
    }

    // 2. Confirmation.
    let tm = env.time_machine_running();
    let herd = env.herd_services_running();
    let cfg = config::get();
    let volumes = match &cfg.backup {
        Some(b) => format!("The “{}” and “{b}” volumes", cfg.workspace),
        None => format!("The “{}” volume", cfg.workspace),
    };
    let mut lines = vec![format!(
        "{volumes} (and any other volume on the SSD) will be unmounted, then the SSD is safe to unplug."
    )];
    if herd {
        lines.push("• Herd will be stopped (.test sites go offline).".into());
    }
    if tm {
        lines.push(
            "• ⚠️ A Time Machine backup is in progress and will be STOPPED. The next backup resumes automatically."
                .into(),
        );
    }
    if dry_run {
        lines.push("\n🧪 Dry run: no commands will actually be executed.".into());
    }
    let ok = if tm { "Stop Backup & Eject" } else { "Eject" };
    if !env.confirm("Eject SSD Workspace?", &lines.join("\n"), ok, tm) {
        return Outcome::Cancelled;
    }

    let mut planned: Vec<String> = Vec::new();

    // 3a. Time Machine.
    if tm {
        if dry_run {
            planned.push(format!(
                "{TMUTIL} stopbackup  (then wait up to 30 seconds for it to stop)"
            ));
        } else {
            env.stop_backup();
        }
    }

    // 3b. Herd — ONLY when its services are running (re-checked right before acting).
    let herd_stopped = env.herd_services_running() && env.herd_cli_available();
    if herd_stopped {
        let app_was_open = env.herd_app_running();
        if dry_run {
            let mut cmd = format!("{} stop", env.herd_cli_display());
            if !app_was_open {
                cmd.push_str("  (then quit the Herd app again if it got launched)");
            }
            planned.push(cmd);
        } else {
            env.herd_stop(app_was_open);
        }
    }

    // 3c. Processes still using the SSD → list them, "Try Again" / "Cancel". Never kill.
    loop {
        let procs = env.busy_processes();
        if procs.is_empty() {
            break;
        }
        let mut text = format!(
            "Close the following apps, terminals, or dev servers (or 'cd' out of the SSD):\n\n{}",
            busy_list_text(&procs)
        );
        if herd_stopped {
            if dry_run {
                text.push_str("\n\n🧪 Dry run: Herd was not actually stopped.");
            } else {
                text.push_str(
                    "\n\nHerd has been stopped. If you cancel the eject and need Herd, run: herd restart",
                );
            }
        }
        if !env.confirm("SSD is still in use", &text, "Try Again", true) {
            return Outcome::CancelledWhileBusy;
        }
    }

    // 3d. Eject.
    if dry_run {
        planned.push(format!("{DISKUTIL} eject {}", system::workspace_path()));
        let list = planned
            .iter()
            .map(|c| format!("• {c}"))
            .collect::<Vec<_>>()
            .join("\n");
        env.alert(
            "🧪 Dry run finished — nothing was executed",
            &format!("Commands that WOULD be run:\n\n{list}"),
            false,
        );
        return Outcome::DryRun(planned);
    }

    let r = env.eject();
    if r.status == 0 {
        env.alert(
            "✓ Safe to unplug",
            "SSD Workspace has been ejected. You can unplug the cable.",
            false,
        );
        Outcome::Ejected
    } else {
        env.alert(
            "Eject failed",
            &format!(
                "Usually an app has just opened a file on the SSD. Wait a moment and try again.\n\n{}",
                r.output.trim()
            ),
            true,
        );
        Outcome::EjectFailed
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// Mock Env: records every dialog and action.
    #[derive(Default)]
    pub struct Mock {
        pub mounted: bool,
        pub tm: bool,
        pub herd: bool,
        pub herd_app: bool,
        /// Successive busy_processes() results; empty once exhausted.
        pub busy: RefCell<VecDeque<Vec<BusyProcess>>>,
        /// Successive confirm() answers; false (Cancel) once exhausted.
        pub answers: RefCell<VecDeque<bool>>,
        pub eject_status: i32,
        pub log: RefCell<Vec<String>>,
    }

    impl Mock {
        fn log(&self, s: String) {
            self.log.borrow_mut().push(s);
        }
        pub fn actions(&self) -> Vec<String> {
            self.log
                .borrow()
                .iter()
                .filter(|l| l.starts_with("ACTION"))
                .cloned()
                .collect()
        }
        pub fn dialogs(&self) -> Vec<String> {
            self.log
                .borrow()
                .iter()
                .filter(|l| !l.starts_with("ACTION"))
                .cloned()
                .collect()
        }
    }

    impl Env for Mock {
        fn workspace_mounted(&self) -> bool {
            self.mounted
        }
        fn time_machine_running(&self) -> bool {
            self.tm
        }
        fn herd_services_running(&self) -> bool {
            self.herd
        }
        fn herd_cli_available(&self) -> bool {
            true
        }
        fn herd_app_running(&self) -> bool {
            self.herd_app
        }
        fn busy_processes(&self) -> Vec<BusyProcess> {
            self.busy.borrow_mut().pop_front().unwrap_or_default()
        }
        fn herd_cli_display(&self) -> String {
            "herd".into()
        }
        fn confirm(&self, title: &str, text: &str, ok: &str, _w: bool) -> bool {
            self.log(format!("CONFIRM {title} [{ok}]\n{text}"));
            self.answers.borrow_mut().pop_front().unwrap_or(false)
        }
        fn alert(&self, title: &str, text: &str, _w: bool) {
            self.log(format!("ALERT {title}\n{text}"));
        }
        fn stop_backup(&self) {
            self.log("ACTION stopbackup".into());
        }
        fn herd_stop(&self, app_was_open: bool) {
            self.log(format!("ACTION herd stop app_was_open={app_was_open}"));
        }
        fn eject(&self) -> CmdOutput {
            self.log("ACTION eject".into());
            CmdOutput {
                status: self.eject_status,
                output: "Volume failed to eject".into(),
            }
        }
    }

    fn p(name: &str, pid: u32) -> BusyProcess {
        BusyProcess {
            name: name.into(),
            pid,
        }
    }

    fn mock(answers: &[bool]) -> Mock {
        Mock {
            mounted: true,
            answers: RefCell::new(answers.iter().copied().collect()),
            ..Default::default()
        }
    }

    #[test]
    fn not_mounted_only_informs() {
        let m = Mock::default();
        assert_eq!(eject_flow(&m, false), Outcome::NotMounted);
        assert!(m.actions().is_empty());
        assert!(m.dialogs()[0].contains("not mounted"));
    }

    #[test]
    fn cancel_at_confirmation_does_nothing() {
        let m = mock(&[false]);
        assert_eq!(eject_flow(&m, false), Outcome::Cancelled);
        assert!(m.actions().is_empty());
    }

    #[test]
    fn free_ssd_ejects_after_confirmation_without_touching_herd() {
        let m = mock(&[true]);
        assert_eq!(eject_flow(&m, false), Outcome::Ejected);
        assert_eq!(m.actions(), vec!["ACTION eject"]);
        let d = m.dialogs();
        assert!(d[0].contains("[Eject]"));
        assert!(d[0].contains("“Workspace” and “Backup” volumes"));
        assert!(!d[0].contains("Herd"), "Herd is off: do not mention it");
        assert!(d[1].contains("✓ Safe to unplug"));
    }

    #[test]
    fn herd_running_is_stopped_and_mentioned() {
        let mut m = mock(&[true]);
        m.herd = true;
        eject_flow(&m, false);
        assert_eq!(
            m.actions(),
            vec!["ACTION herd stop app_was_open=false", "ACTION eject"]
        );
        assert!(m.dialogs()[0].contains("Herd will be stopped"));
    }

    #[test]
    fn time_machine_is_stopped_with_warning_button() {
        let mut m = mock(&[true]);
        m.tm = true;
        eject_flow(&m, false);
        assert_eq!(m.actions(), vec!["ACTION stopbackup", "ACTION eject"]);
        let d = m.dialogs();
        assert!(d[0].contains("[Stop Backup & Eject]"));
        assert!(d[0].contains("STOPPED"));
    }

    #[test]
    fn busy_processes_retry_then_eject() {
        let m = mock(&[true, true]);
        m.busy
            .borrow_mut()
            .push_back(vec![p("node", 10), p("zsh", 11)]);
        assert_eq!(eject_flow(&m, false), Outcome::Ejected);
        let d = m.dialogs();
        assert!(d[1].contains("SSD is still in use [Try Again]"));
        assert!(d[1].contains("• node  (PID 10)"));
        assert!(!d[1].contains("Herd has been stopped"));
        assert_eq!(m.actions(), vec!["ACTION eject"]);
    }

    #[test]
    fn busy_processes_cancel_never_ejects() {
        let mut m = mock(&[true, false]);
        m.herd = true;
        m.busy.borrow_mut().push_back(vec![p("zsh", 11)]);
        assert_eq!(eject_flow(&m, false), Outcome::CancelledWhileBusy);
        assert!(!m.actions().contains(&"ACTION eject".to_string()));
        assert!(m.dialogs()[1].contains("Herd has been stopped"));
    }

    #[test]
    fn busy_list_is_truncated_at_12() {
        let procs: Vec<_> = (0..15).map(|i| p(&format!("p{i}"), i)).collect();
        let t = busy_list_text(&procs);
        assert_eq!(t.lines().count(), 13);
        assert!(t.ends_with("…and 3 more"));
    }

    #[test]
    fn eject_failure_shows_diskutil_output() {
        let mut m = mock(&[true]);
        m.eject_status = 1;
        assert_eq!(eject_flow(&m, false), Outcome::EjectFailed);
        assert!(m.dialogs()[1].contains("Volume failed to eject"));
    }

    #[test]
    fn dry_run_never_performs_actions() {
        let mut m = mock(&[true, true]);
        m.tm = true;
        m.herd = true;
        m.busy.borrow_mut().push_back(vec![p("zsh", 11)]);
        let out = eject_flow(&m, true);
        assert!(
            m.actions().is_empty(),
            "a dry run must not act: {:?}",
            m.actions()
        );
        let Outcome::DryRun(planned) = out else {
            panic!("must be DryRun")
        };
        assert_eq!(planned.len(), 3);
        assert!(planned[0].starts_with("/usr/bin/tmutil stopbackup"));
        assert!(planned[1].starts_with("herd stop"));
        assert_eq!(planned[2], "/usr/sbin/diskutil eject /Volumes/Workspace");
        let d = m.dialogs();
        assert!(d[0].contains("Dry run: no commands"));
        assert!(d[1].contains("Dry run: Herd was not actually stopped"));
        assert!(d.last().unwrap().contains("WOULD be run"));
    }

    #[test]
    fn dry_run_with_herd_off_does_not_plan_herd() {
        let m = mock(&[true]);
        let Outcome::DryRun(planned) = eject_flow(&m, true) else {
            panic!()
        };
        assert_eq!(planned, vec!["/usr/sbin/diskutil eject /Volumes/Workspace"]);
    }
}

/// Dry-run test using the REAL probes (lsof, tmutil, process table, mounts) with automatic
/// dialog answers. Every action that would change the system `panic!`s, so it is safe to run
/// on the SSD itself.
///
/// ```sh
/// cargo test real_system_dry_run -- --ignored --nocapture
/// SSD_WORKSPACE_SIMULATE=tm,herd,idle cargo test real_system_dry_run -- --ignored --nocapture
/// ```
#[cfg(test)]
mod real_system {
    use super::*;
    use crate::system;
    use std::cell::Cell;

    struct RealProbes {
        sim_tm: bool,
        sim_herd: bool,
        sim_idle: bool,
        retries_left: Cell<u32>,
        confirms: Cell<u32>,
    }

    impl Env for RealProbes {
        fn workspace_mounted(&self) -> bool {
            system::is_mounted(system::workspace_path())
        }
        fn time_machine_running(&self) -> bool {
            self.sim_tm || system::time_machine_running()
        }
        fn herd_services_running(&self) -> bool {
            self.sim_herd || system::herd_services_running()
        }
        fn herd_cli_available(&self) -> bool {
            self.sim_herd || system::herd_cli_path().is_file()
        }
        fn herd_app_running(&self) -> bool {
            system::herd_app_running()
        }
        fn busy_processes(&self) -> Vec<BusyProcess> {
            if self.sim_idle {
                Vec::new()
            } else {
                system::busy_processes()
            }
        }
        fn herd_cli_display(&self) -> String {
            system::herd_cli_path().display().to_string()
        }
        fn confirm(&self, title: &str, text: &str, ok: &str, _w: bool) -> bool {
            self.confirms.set(self.confirms.get() + 1);
            println!("\n┌─ DIALOG: {title}   [{ok}] [Cancel]\n{text}\n└─");
            // First confirmation: "Eject". "Try Again": once, then Cancel.
            if title == "SSD is still in use" {
                let left = self.retries_left.get();
                self.retries_left.set(left.saturating_sub(1));
                let answer = left > 0;
                println!("   → {}", if answer { ok } else { "Cancel" });
                return answer;
            }
            println!("   → {ok}");
            true
        }
        fn alert(&self, title: &str, text: &str, _w: bool) {
            println!("\n┌─ INFO: {title}\n{text}\n└─");
        }
        fn stop_backup(&self) {
            panic!("dry run called stop_backup!");
        }
        fn herd_stop(&self, _: bool) {
            panic!("dry run called herd_stop!");
        }
        fn eject(&self) -> CmdOutput {
            panic!("dry run called eject!");
        }
    }

    #[test]
    #[ignore = "uses the real system; run manually"]
    fn real_system_dry_run() {
        let sim = std::env::var("SSD_WORKSPACE_SIMULATE").unwrap_or_default();
        let has = |k: &str| sim.split(',').any(|s| s.trim() == k);
        let herd_before = system::herd_services_running();
        let herd_app_before = system::herd_app_running();
        let env = RealProbes {
            sim_tm: has("tm"),
            sim_herd: has("herd"),
            sim_idle: has("idle"),
            retries_left: Cell::new(1),
            confirms: Cell::new(0),
        };
        let out = eject_flow(&env, true);
        println!("\nRESULT: {out:?}");
        assert!(
            system::is_mounted(system::workspace_path()),
            "the SSD must stay mounted"
        );
        assert_eq!(
            system::herd_services_running(),
            herd_before,
            "the Herd state must not change"
        );
        assert_eq!(
            system::herd_app_running(),
            herd_app_before,
            "the Herd app must not be launched or quit"
        );
    }
}
