//! SSD Workspace: a macOS menu bar app that shows what holds an external SSD and ejects it
//! safely. This file holds the app state, the monitor thread, dialogs, the Detail window and
//! the commands the frontend calls; see docs/ARCHITECTURE.md for the overall design.

mod config;
mod eject;
mod holders;
mod menu_icons;
mod menu_style;
mod system;
pub mod timemachine;
mod tray;

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{
    AppHandle, Emitter, LogicalPosition, Manager, RunEvent, TitleBarStyle, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use system::{BusyProcess, CmdOutput, Space};
use timemachine::TmInfo;

pub const DETAIL_LABEL: &str = "detail";
const DETAIL_HEIGHT: f64 = 560.0;
/// How long a closed Detail window is kept (hidden) so reopening it is instant.
const DETAIL_KEEP_ALIVE: Duration = Duration::from_secs(120);
/// Show the Detail window after this long even if the page has not reported ready.
const DETAIL_READY_TIMEOUT: Duration = Duration::from_millis(1500);
/// Bumped on every show, so a pending teardown of a hidden window can tell it was reopened.
static DETAIL_GENERATION: AtomicU64 = AtomicU64::new(0);

/// How often Time Machine is checked while a backup or an eject is in progress (live progress).
const ACTIVE_POLL: Duration = Duration::from_secs(3);
/// How often Time Machine is checked the rest of the time, to notice a backup that started.
const IDLE_POLL: Duration = Duration::from_secs(30);
/// How often `/Volumes` is checked when mount notifications are unavailable.
const FALLBACK_MOUNT_POLL: Duration = Duration::from_secs(3);
/// The Time Machine preferences only change when a backup finishes; re-read them this often anyway.
const PREFS_MAX_AGE: Duration = Duration::from_secs(600);
/// Let a burst of mount notifications (two volumes on one SSD) settle before refreshing.
const MOUNT_SETTLE: Duration = Duration::from_millis(250);

// ---------------------------------------------------------------------------------------
// State

/// Simulation for testing the eject flow. ONLY applies while dry-run mode is on.
/// Read from `SSD_WORKSPACE_SIMULATE`, e.g. `tm,herd` or `idle`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Simulate {
    /// Pretend Time Machine is backing up.
    tm: bool,
    /// Pretend the Herd services are running.
    herd: bool,
    /// Pretend no process is using the SSD.
    idle: bool,
}

impl Simulate {
    fn from_env() -> Self {
        let v = std::env::var("SSD_WORKSPACE_SIMULATE").unwrap_or_default();
        let has = |k: &str| v.split(',').any(|s| s.trim() == k);
        Simulate {
            tm: has("tm"),
            herd: has("herd"),
            idle: has("idle"),
        }
    }
}

/// Status summary for the tray menu (compared so the menu is only rebuilt when it changed).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub workspace_mounted: bool,
    pub backup_mounted: bool,
    pub workspace_space: Option<Space>,
    pub tm: TmInfo,
    pub busy: bool,
    pub autostart: bool,
    pub dry_run: bool,
}

impl Snapshot {
    /// Time Machine is writing to the backup volume on the SSD, so ejecting now would cut
    /// the backup short.
    pub fn backing_up_to_ssd(&self) -> bool {
        let Some(backup) = config::get().backup.as_deref() else {
            return false;
        };
        self.tm.running
            && self.backup_mounted
            && self.tm.destination.as_deref().is_none_or(|d| d == backup)
    }
}

/// Why the monitor woke up, ordered by how much has to be re-read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Wake {
    /// The poll interval elapsed.
    Tick,
    /// A volume was mounted or unmounted.
    Volumes,
    /// Something asked for fresh status (menu, Detail window, an action that just finished).
    Refresh,
}

pub struct AppState {
    snapshot: Mutex<Option<Snapshot>>,
    busy: AtomicBool,
    dry_run: AtomicBool,
    simulate: Simulate,
    wake: Sender<Wake>,
}

impl AppState {
    fn new(wake: Sender<Wake>) -> Self {
        // Debug builds always start in dry-run mode (this project itself lives on the SSD).
        let dry_run = cfg!(debug_assertions)
            || std::env::var("SSD_WORKSPACE_DRY_RUN").is_ok_and(|v| v == "1");
        AppState {
            snapshot: Mutex::new(None),
            busy: AtomicBool::new(false),
            dry_run: AtomicBool::new(dry_run),
            simulate: Simulate::from_env(),
            wake,
        }
    }
    pub fn dry_run(&self) -> bool {
        self.dry_run.load(Ordering::SeqCst)
    }
    fn sim(&self) -> Simulate {
        if self.dry_run() {
            self.simulate
        } else {
            Simulate::default()
        }
    }
    /// Apply the `tm` simulation: a 90-second backup that repeats (preparing, copying with
    /// progress and an estimate, finishing).
    fn simulate_tm(&self, mut info: TmInfo) -> TmInfo {
        if self.sim().tm && !info.running {
            static START: OnceLock<Instant> = OnceLock::new();
            let t = START.get_or_init(Instant::now).elapsed().as_secs() % 90;
            let total: u64 = 4_200_000_000;
            let progress = match t {
                0..10 => timemachine::Progress::default(),
                10..80 => {
                    let f = (t - 10) as f64 / 70.0;
                    timemachine::Progress {
                        phase: timemachine::Phase::Copying,
                        fraction: Some(f),
                        bytes: Some((total as f64 * f) as u64),
                        total_bytes: Some(total),
                        time_remaining: Some((80 - t) * 6),
                    }
                }
                _ => timemachine::Progress {
                    phase: timemachine::Phase::Finishing,
                    ..Default::default()
                },
            };
            info.running = true;
            info.progress = Some(progress);
            info.status_text = timemachine::status_text(&info);
            info.remaining_text = timemachine::remaining_text(&info);
        }
        info
    }
    /// The Time Machine info from the last refresh (read fresh if there was none yet).
    fn tm_info(&self) -> TmInfo {
        let cached = self.snapshot.lock().unwrap().as_ref().map(|s| s.tm.clone());
        cached.unwrap_or_else(|| self.simulate_tm(timemachine::info()))
    }
    fn herd_running(&self) -> bool {
        self.sim().herd || system::herd_services_running()
    }
}

fn state(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

// ---------------------------------------------------------------------------------------
// Monitor
//
// One background thread owns all status reading. It sleeps until a volume is mounted or
// unmounted (kernel notification), someone asks for a refresh, or the poll interval elapses.
// Mount state, free space and Herd detection run in-process; the only child process in the
// steady state is one `tmutil status` per interval.

/// Ask the monitor to re-read everything now. Cheap and non-blocking; the result reaches
/// the tray and the Detail window (event `status-changed`) shortly after.
pub fn request_refresh(app: &AppHandle) {
    let _ = state(app).wake.send(Wake::Refresh);
}

struct Monitor {
    status: timemachine::Status,
    status_at: Instant,
    prefs: timemachine::Prefs,
    prefs_at: Option<Instant>,
    /// Preference reads still owed after a backup finished (the snapshot date can land late).
    prefs_rereads: u8,
    /// A backup or an eject is in progress: poll fast.
    active: bool,
    /// `SSD_WORKSPACE_TRACE=1`: log every refresh and what it had to read.
    trace: bool,
}

impl Monitor {
    fn new() -> Self {
        Monitor {
            status: (false, None),
            status_at: Instant::now(),
            prefs: timemachine::Prefs::default(),
            prefs_at: None,
            prefs_rereads: 0,
            active: false,
            trace: std::env::var("SSD_WORKSPACE_TRACE").is_ok_and(|v| v == "1"),
        }
    }

    /// Re-read what `wake` calls for, update the tray if anything changed, and push the new
    /// status to the Detail window if it is open.
    fn refresh(&mut self, app: &AppHandle, wake: Wake) {
        let st = state(app);
        let now = Instant::now();

        let status_due =
            wake > Wake::Tick || self.active || now.duration_since(self.status_at) >= IDLE_POLL;
        if status_due {
            let was_running = self.status.0;
            self.status = timemachine::read_status();
            self.status_at = now;
            if was_running && !self.status.0 {
                self.prefs_rereads = 2;
            }
        }
        let prefs_stale = wake == Wake::Refresh
            || self.prefs_rereads > 0
            || self
                .prefs_at
                .is_none_or(|at| now.duration_since(at) >= PREFS_MAX_AGE);
        if prefs_stale {
            self.prefs = timemachine::read_prefs();
            self.prefs_at = Some(now);
            self.prefs_rereads = self.prefs_rereads.saturating_sub(1);
        }
        if self.trace {
            eprintln!(
                "[ssd-workspace] refresh: {wake:?} (tmutil: {status_due}, defaults: {prefs_stale})"
            );
        }

        let workspace_mounted = system::is_mounted(system::workspace_path());
        let snap = Snapshot {
            workspace_mounted,
            backup_mounted: system::backup_path().is_some_and(system::is_mounted),
            workspace_space: workspace_mounted
                .then(|| system::space_info(system::workspace_path()))
                .flatten(),
            tm: st.simulate_tm(timemachine::build_info(&self.prefs, self.status.clone())),
            busy: st.busy.load(Ordering::SeqCst),
            autostart: app.autolaunch().is_enabled().unwrap_or(false),
            dry_run: st.dry_run(),
        };
        self.active = snap.tm.running || snap.busy;

        let changed = {
            let mut cur = st.snapshot.lock().unwrap();
            let changed = cur.as_ref() != Some(&snap);
            if changed {
                *cur = Some(snap.clone());
            }
            changed
        };
        if changed {
            tray::update(app, &snap);
        }
        if app
            .get_webview_window(DETAIL_LABEL)
            .is_some_and(|w| w.is_visible().unwrap_or(false))
        {
            let _ = app.emit("status-changed", DetailStatus::collect(&st, snap.tm));
        }
    }
}

fn start_monitor(app: AppHandle, wakes: Receiver<Wake>) {
    thread::spawn(move || {
        let tx = state(&app).wake.clone();
        let watching = system::watch_mounts(move || tx.send(Wake::Volumes).is_ok());
        if !watching {
            eprintln!("[ssd-workspace] mount notifications unavailable; polling /Volumes");
        }
        let mut monitor = Monitor::new();
        let mut wake = Wake::Refresh;
        loop {
            monitor.refresh(&app, wake);
            let timeout = if monitor.active {
                ACTIVE_POLL
            } else if watching {
                IDLE_POLL
            } else {
                FALLBACK_MOUNT_POLL
            };
            wake = match wakes.recv_timeout(timeout) {
                Ok(first) => {
                    if first == Wake::Volumes {
                        thread::sleep(MOUNT_SETTLE);
                    }
                    // Several requests in a row need only one refresh.
                    wakes.try_iter().fold(first, Wake::max)
                }
                Err(RecvTimeoutError::Timeout) => Wake::Tick,
                Err(RecvTimeoutError::Disconnected) => return,
            };
        }
    });
}

// ---------------------------------------------------------------------------------------
// macOS: activate the app so dialogs and windows come to the front (we are an "Accessory" app).

pub fn activate_app(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| {
        if let Some(mtm) = objc2::MainThreadMarker::new() {
            let ns_app = objc2_app_kit::NSApplication::sharedApplication(mtm);
            #[allow(deprecated)]
            ns_app.activateIgnoringOtherApps(true);
        }
    });
}

// ---------------------------------------------------------------------------------------
// Eject

/// [`eject::Env`] implemented on the real system plus Tauri dialogs.
struct TauriEnv<'a> {
    app: &'a AppHandle,
    sim: Simulate,
}

impl TauriEnv<'_> {
    fn dialog(
        &self,
        title: &str,
        text: &str,
        warning: bool,
    ) -> tauri_plugin_dialog::MessageDialogBuilder<tauri::Wry> {
        activate_app(self.app);
        self.app
            .dialog()
            .message(text)
            .title(title)
            .kind(if warning {
                MessageDialogKind::Warning
            } else {
                MessageDialogKind::Info
            })
    }
}

impl eject::Env for TauriEnv<'_> {
    fn workspace_mounted(&self) -> bool {
        system::is_mounted(system::workspace_path())
    }
    fn time_machine_running(&self) -> bool {
        self.sim.tm || system::time_machine_running()
    }
    fn herd_services_running(&self) -> bool {
        self.sim.herd || system::herd_services_running()
    }
    fn herd_cli_available(&self) -> bool {
        // While simulating Herd, pretend the CLI exists so the `herd stop` plan shows up too.
        self.sim.herd || system::herd_cli_path().is_file()
    }
    fn herd_app_running(&self) -> bool {
        system::herd_app_running()
    }
    fn busy_processes(&self) -> Vec<BusyProcess> {
        if self.sim.idle {
            Vec::new()
        } else {
            system::busy_processes()
        }
    }
    fn herd_cli_display(&self) -> String {
        system::herd_cli_path().display().to_string()
    }
    fn confirm(&self, title: &str, text: &str, ok: &str, warning: bool) -> bool {
        self.dialog(title, text, warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                ok.to_string(),
                "Cancel".to_string(),
            ))
            .blocking_show()
    }
    fn alert(&self, title: &str, text: &str, warning: bool) {
        self.dialog(title, text, warning)
            .buttons(MessageDialogButtons::OkCustom("OK".to_string()))
            .blocking_show();
    }
    fn stop_backup(&self) {
        system::run(system::TMUTIL, &["stopbackup"]);
        for _ in 0..30 {
            if !system::time_machine_running() {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }
    fn herd_stop(&self, app_was_open: bool) {
        system::herd_stop(app_was_open);
    }
    fn eject(&self) -> CmdOutput {
        system::run(system::DISKUTIL, &["eject", system::workspace_path()])
    }
}

/// Start the eject flow on a background thread (the UI stays responsive). Only one at a time.
pub fn start_eject(app: &AppHandle) {
    let st = state(app);
    // Eject is disabled while Time Machine backs up to the SSD (menu and Detail window).
    let backing_up = st
        .snapshot
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(Snapshot::backing_up_to_ssd);
    if backing_up {
        return;
    }
    if st.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let dry_run = st.dry_run();
    let sim = st.sim();
    let app = app.clone();
    request_refresh(&app);
    thread::spawn(move || {
        let env = TauriEnv { app: &app, sim };
        let outcome = eject::eject_flow(&env, dry_run);
        eprintln!("[ssd-workspace] eject finished (dry_run={dry_run}): {outcome:?}");
        state(&app).busy.store(false, Ordering::SeqCst);
        request_refresh(&app);
    });
}

pub fn toggle_autostart(app: &AppHandle) {
    let al = app.autolaunch();
    let res = if al.is_enabled().unwrap_or(false) {
        al.disable()
    } else {
        al.enable()
    };
    if let Err(e) = res {
        let app2 = app.clone();
        thread::spawn(move || {
            activate_app(&app2);
            app2.dialog()
                .message(format!(
                    "{e}\n\nAdd it manually in System Settings → General → Login Items."
                ))
                .title("Couldn’t change the login setting")
                .kind(MessageDialogKind::Warning)
                .blocking_show();
        });
    }
    request_refresh(app);
}

pub fn toggle_dry_run(app: &AppHandle) {
    let st = state(app);
    st.dry_run.fetch_xor(true, Ordering::SeqCst);
    request_refresh(app);
}

// ---------------------------------------------------------------------------------------
// Time Machine (replaces the built-in Time Machine menu)

fn warn(app: &AppHandle, title: &str, text: String) {
    activate_app(app);
    app.dialog()
        .message(text)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .blocking_show();
}

/// "Back Up Now". Only when no backup is running.
pub fn tm_start_backup(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        if !system::time_machine_running() {
            let r = timemachine::start_backup();
            if r.status != 0 {
                warn(
                    &app,
                    "Couldn’t start the backup",
                    format!(
                        "Make sure the backup disk is connected.\n\n{}",
                        r.output.trim()
                    ),
                );
            }
        }
        // Give backupd a moment to start, then show the first phase.
        thread::sleep(Duration::from_millis(800));
        request_refresh(&app);
    });
}

/// "Skip This Backup". Only while a backup is running.
pub fn tm_skip_backup(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        if system::time_machine_running() {
            let r = timemachine::skip_backup();
            if r.status != 0 {
                warn(
                    &app,
                    "Couldn’t skip the backup",
                    r.output.trim().to_string(),
                );
            }
        }
        thread::sleep(Duration::from_millis(800));
        request_refresh(&app);
    });
}

// ---------------------------------------------------------------------------------------
// Detail window

pub fn show_detail(app: &AppHandle) {
    activate_app(app);
    // Every show cancels a pending teardown of a hidden window.
    DETAIL_GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(DETAIL_LABEL) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    // Native Liquid Glass (macOS 26+, NSGlassEffectView); vibrancy fallback on older macOS.
    // The webview is transparent so the glass shows through; the title is hidden and the
    // traffic lights float.
    //
    // The window starts hidden and appears once the page has rendered (`detail_ready`), so
    // it never flashes as empty glass while the web process starts.
    let effects = EffectsBuilder::new()
        .effects([Effect::LiquidGlassRegular, Effect::UnderWindowBackground])
        .state(EffectState::FollowsWindowActiveState)
        .build();
    let built = WebviewWindowBuilder::new(app, DETAIL_LABEL, WebviewUrl::App("index.html".into()))
        .title("SSD Workspace")
        .inner_size(360.0, DETAIL_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .transparent(true)
        .effects(effects)
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(LogicalPosition::new(18.0, 25.0))
        .center()
        .visible(false)
        .build();
    let w = match built {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[ssd-workspace] failed to open the Detail window: {e}");
            return;
        }
    };
    // Closing only hides the window, so opening it again shortly after is instant. It is
    // destroyed (with its web process) once it has stayed hidden for DETAIL_KEEP_ALIVE.
    let hidden = w.clone();
    w.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = hidden.hide();
            let generation = DETAIL_GENERATION.load(Ordering::SeqCst);
            let w = hidden.clone();
            thread::spawn(move || {
                thread::sleep(DETAIL_KEEP_ALIVE);
                if DETAIL_GENERATION.load(Ordering::SeqCst) == generation
                    && !w.is_visible().unwrap_or(true)
                {
                    let _ = w.destroy();
                }
            });
        }
    });
    // Safety net: show the window even if the page never reports that it is ready.
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(DETAIL_READY_TIMEOUT);
        reveal_detail(&app);
    });
}

/// Show the (hidden) Detail window once; does nothing if it is already visible.
fn reveal_detail(app: &AppHandle) {
    if let Some(w) = app
        .get_webview_window(DETAIL_LABEL)
        .filter(|w| !w.is_visible().unwrap_or(true))
    {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

// ---------------------------------------------------------------------------------------
// Commands for the Detail window

#[derive(Clone, Serialize)]
struct VolumeStatus {
    /// Volume name from the config, e.g. "Workspace".
    name: String,
    mounted: bool,
    space: Option<Space>,
}

impl VolumeStatus {
    fn of(name: &str, path: &str) -> Self {
        let mounted = system::is_mounted(path);
        VolumeStatus {
            name: name.to_string(),
            mounted,
            space: mounted.then(|| system::space_info(path)).flatten(),
        }
    }
}

/// Everything the Detail window shows. Also the payload of the `status-changed` event, so
/// the window never has to ask again after being told something changed.
#[derive(Clone, Serialize)]
struct DetailStatus {
    workspace: VolumeStatus,
    /// `None` when the config has no backup volume.
    backup: Option<VolumeStatus>,
    tm: TmInfo,
    herd_running: bool,
    busy: bool,
    dry_run: bool,
    storage: Option<holders::Storage>,
    /// Only in the first answer (`get_status`): the last known apps holding the SSD, so the
    /// window opens with a verdict instead of "Checking…". Events leave it out (icons are big).
    #[serde(skip_serializing_if = "Option::is_none")]
    holders: Option<Vec<holders::Holder>>,
}

impl DetailStatus {
    /// Runs no child process: volumes, storage and Herd are read in-process.
    fn collect(st: &AppState, tm: TmInfo) -> Self {
        let cfg = config::get();
        DetailStatus {
            workspace: VolumeStatus::of(&cfg.workspace, cfg.workspace_path()),
            backup: cfg
                .backup
                .as_deref()
                .zip(cfg.backup_path())
                .map(|(name, path)| VolumeStatus::of(name, path)),
            tm,
            herd_running: st.herd_running(),
            busy: st.busy.load(Ordering::SeqCst),
            dry_run: st.dry_run(),
            storage: holders::storage(),
            holders: None,
        }
    }
}

/// Current status, answered right away from the last refresh. It also asks the monitor for
/// a fresh reading, which arrives as a `status-changed` event.
#[tauri::command]
async fn get_status(app: AppHandle) -> Result<DetailStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = state(&app);
        let mut status = DetailStatus::collect(&st, st.tm_info());
        status.holders = if st.sim().idle {
            Some(Vec::new())
        } else {
            holders::last()
        };
        request_refresh(&app);
        status
    })
    .await
    .map_err(|e| e.to_string())
}

/// Apps (with icon and folder) still holding the SSD. Runs lsof, so only on request.
#[tauri::command]
async fn list_holders(app: AppHandle) -> Result<Vec<holders::Holder>, String> {
    tauri::async_runtime::spawn_blocking(move || list_holders_for(&app))
        .await
        .map_err(|e| e.to_string())
}

/// Apps holding the SSD; empty under the dry-run `idle` simulation.
pub(crate) fn list_holders_for(app: &AppHandle) -> Vec<holders::Holder> {
    if state(app).sim().idle {
        return Vec::new();
    }
    holders::list()
}

/// The page has rendered its first status: show the window.
#[tauri::command]
fn detail_ready(app: AppHandle) {
    reveal_detail(&app);
}

/// Bring an app that is holding the SSD to the front (it is never quit or signalled).
#[tauri::command]
fn show_app(bundle: String) -> Result<(), String> {
    holders::show_app(&bundle)
}

#[tauri::command]
fn eject_ssd(app: AppHandle) {
    start_eject(&app);
}

#[tauri::command]
fn time_machine_action(app: AppHandle, action: String) -> Result<(), String> {
    match action.as_str() {
        "start" => tm_start_backup(&app),
        "skip" => tm_skip_backup(&app),
        "browse" => timemachine::browse(),
        "settings" => timemachine::open_settings(),
        _ => return Err(format!("unknown action: {action}")),
    }
    Ok(())
}

#[tauri::command]
fn refresh_status(app: AppHandle) {
    request_refresh(&app);
}

#[tauri::command]
fn open_workspace() {
    if system::is_mounted(system::workspace_path()) {
        system::open_in_finder(system::workspace_path());
    }
}

// ---------------------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Never hold the SSD through the working directory (e.g. when launched from a terminal
    // on the SSD): this app's own process is excluded from the lsof list, so it must really
    // let go.
    let _ = std::env::set_current_dir("/");
    system::exit_on_vanished_code();
    let context = tauri::generate_context!();
    config::load(&context.config().identifier);
    let (wake, wakes) = mpsc::channel();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .macos_launcher(tauri_plugin_autostart::MacosLauncher::LaunchAgent)
                .build(),
        )
        .manage(AppState::new(wake))
        .invoke_handler(tauri::generate_handler![
            get_status,
            list_holders,
            detail_ready,
            show_app,
            eject_ssd,
            open_workspace,
            time_machine_action,
            refresh_status
        ])
        .setup(|app| {
            // Menu bar icon only: no Dock icon, no window at startup.
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::create(app.handle())?;
            start_monitor(app.handle().clone(), wakes);
            Ok(())
        })
        .build(context)
        .expect("failed to start SSD Workspace");

    app.run(|_app, event| {
        // Closing the Detail window must not end the app; only "Quit" does (it has an exit code).
        if let RunEvent::ExitRequested {
            api, code: None, ..
        } = event
        {
            api.prevent_exit();
        }
    });
}
