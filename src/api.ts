/**
 * Typed wrappers for the Rust commands and events the Detail window uses. This is the whole
 * interface between the web view and the app: the frontend never touches the system itself.
 * The Rust side lives in `src-tauri/src/lib.rs` (commands) and `holders.rs`.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Space {
  free: number;
  total: number;
}

export interface Volume {
  /** Volume name from the config, e.g. "Workspace". */
  name: string;
  mounted: boolean;
  space: Space | null;
}

export type TmPhase = "preparing" | "copying" | "finishing" | "stopping";

export interface TmProgress {
  phase: TmPhase;
  /** 0–1 when known. */
  fraction: number | null;
  bytes: number | null;
  total_bytes: number | null;
  /** Time Machine's estimate in seconds, while copying. */
  time_remaining: number | null;
}

export interface TimeMachine {
  configured: boolean;
  destination: string | null;
  latest: number | null;
  /** "Today, 18:09" */
  latest_text: string | null;
  running: boolean;
  progress: TmProgress | null;
  /** Ready-to-show status sentence, e.g. "Backing up: 1.2 GB of 3.4 GB". */
  status_text: string;
  /** "About 3 minutes remaining", while copying. */
  remaining_text: string | null;
}

export interface DetailStatus {
  workspace: Volume;
  /** null when the config has no backup volume. */
  backup: Volume | null;
  tm: TimeMachine;
  herd_running: boolean;
  busy: boolean;
  dry_run: boolean;
  /** APFS container breakdown, read in-process; null when the SSD is not mounted. */
  storage: Storage | null;
  /** Only in `getStatus`: the last known apps holding the SSD (a fresh check follows). */
  holders?: Holder[];
}

/** An app (or a lone process) that still has files open on the SSD. */
export interface Holder {
  /** "Terminal", "Zed", or the process name when there is no app. */
  name: string;
  /** Path of the .app bundle, for `showApp`. */
  bundle: string | null;
  /** PNG data URL of the app icon. */
  icon: string | null;
  /** e.g. "Workspace/Code/my-project" */
  folder: string | null;
  processes: number;
}

/** The APFS container the two volumes share (bytes). */
export interface Storage {
  total: number;
  free: number;
  workspace: number;
  backup: number | null;
}

/**
 * Current status, answered right away from the last reading. Rust then takes a fresh
 * reading, which arrives through `onStatusChanged`.
 */
export const getStatus = () => invoke<DetailStatus>("get_status");

/** Apps still holding the SSD, grouped from lsof (a second or so). */
export const listHolders = () => invoke<Holder[]>("list_holders");

/** The page has rendered: Rust shows the window (it starts hidden to avoid empty glass). */
export const detailReady = () => invoke<void>("detail_ready");

/** Bring an app from `listHolders` to the front. Nothing is ever quit. */
export const showApp = (bundle: string) => invoke<void>("show_app", { bundle });

/** Start the eject flow (Rust shows the confirmation and dialogs). */
export const ejectSsd = () => invoke<void>("eject_ssd");

export const openWorkspace = () => invoke<void>("open_workspace");

export type TmAction = "start" | "skip" | "browse" | "settings";

/** Back Up Now / Skip / Browse / Time Machine Settings. */
export const timeMachine = (action: TmAction) => invoke<void>("time_machine_action", { action });

/** Ask Rust for a fresh reading (the result arrives through `onStatusChanged`). */
export const refreshStatus = () => invoke<void>("refresh_status");

/**
 * Called with the new status whenever Rust takes a reading (plug/unplug, eject, backup
 * progress, refresh). The status travels with the event, so there is nothing to fetch.
 */
export const onStatusChanged = (cb: (status: DetailStatus) => void): Promise<UnlistenFn> =>
  listen<DetailStatus>("status-changed", (event) => cb(event.payload));
