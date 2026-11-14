/**
 * The Detail window. It answers one question: can the SSD be unplugged now?
 *
 * Layout: a verdict at the top, a checklist (apps holding the SSD, Time Machine, Herd), the
 * storage bar, and Open in Finder / Eject. All data comes from Rust (see `api.ts`): the first
 * status from `getStatus`, updates through the `status-changed` event, and the list of apps
 * from `listHolders` (lsof, so only on open, focus, Refresh, and mount changes).
 */
import type { ComponentChildren } from "preact";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import {
  type DetailStatus,
  type Holder,
  detailReady,
  ejectSsd,
  getStatus,
  listHolders,
  onStatusChanged,
  openWorkspace,
  refreshStatus,
  showApp,
  timeMachine,
} from "./api";
import {
  AppIcon,
  CheckIcon,
  DashIcon,
  DriveIcon,
  EjectIcon,
  FolderIcon,
  RefreshIcon,
  StopIcon,
} from "./icons";

const gb = (bytes: number) => `${Math.round(bytes / 1e9)} GB`;
const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

type Tone = "ok" | "warn" | "idle";

/** Time Machine is writing to the backup volume on this SSD (mirrors `backing_up_to_ssd`). */
function backingUpToSsd(s: DetailStatus): boolean {
  const b = s.backup;
  return (
    s.tm.running && !!b?.mounted && (s.tm.destination === null || s.tm.destination === b.name)
  );
}

/** The one question this window answers: can the SSD be ejected right now? */
function verdict(
  s: DetailStatus,
  holders: Holder[] | null,
): { tone: Tone; title: string; sub: string } {
  if (!s.workspace.mounted)
    return { tone: "idle", title: "Not mounted", sub: "Plug in the SSD to start working." };
  if (s.busy) return { tone: "warn", title: "Ejecting…", sub: "Stopping what uses the SSD first." };
  if (holders === null)
    return { tone: "idle", title: "Checking…", sub: "Looking for apps that use the SSD." };
  if (holders.length > 0)
    return {
      tone: "warn",
      title: `Close ${plural(holders.length, "app", "apps")} first`,
      sub: "They still have files open on the SSD.",
    };
  if (backingUpToSsd(s))
    return {
      tone: "warn",
      title: "Backing up",
      sub: "You can eject once Time Machine is done, or skip this backup.",
    };
  const stopped = [s.tm.running && "Time Machine", s.herd_running && "Herd"].filter(Boolean);
  return {
    tone: "ok",
    title: "Ready to eject",
    sub: stopped.length
      ? `${stopped.join(" and ")} will be stopped first.`
      : "Nothing is using the SSD.",
  };
}

/** Status glyph: done, handled automatically on eject, or not applicable. */
function Glyph({ kind }: { kind: "ok" | "auto" | "off" }) {
  return (
    <span className={`glyph ${kind}`} aria-hidden>
      {kind === "ok" ? <CheckIcon /> : kind === "auto" ? <StopIcon /> : <DashIcon />}
    </span>
  );
}

function Item({
  lead,
  title,
  sub,
  trail,
  children,
}: {
  lead: ComponentChildren;
  title: ComponentChildren;
  sub?: ComponentChildren;
  trail?: ComponentChildren;
  children?: ComponentChildren;
}) {
  return (
    <li>
      <div className="row">
        {lead}
        <div className="stack grow-x">
          <span className="title">{title}</span>
          {sub && <span className="sub">{sub}</span>}
        </div>
        {trail}
      </div>
      {children}
    </li>
  );
}

function HolderItem({ h }: { h: Holder }) {
  const folder = h.folder?.split("/").slice(-2).join("/");
  return (
    <Item
      lead={
        <span className="app-icon" aria-hidden>
          {h.icon ? <img src={h.icon} alt="" /> : <AppIcon />}
          <span className="badge-dot" />
        </span>
      }
      title={h.name}
      sub={
        <span title={h.folder ?? undefined}>
          {[folder, h.processes > 1 && plural(h.processes, "process", "processes")]
            .filter(Boolean)
            .join(" · ")}
        </span>
      }
      trail={
        h.bundle && (
          <button className="link" onClick={() => showApp(h.bundle!)}>
            Show
          </button>
        )
      }
    />
  );
}

function TimeMachineItem({ s }: { s: DetailStatus }) {
  const tm = s.tm;
  if (!tm.configured)
    return <Item lead={<Glyph kind="off" />} title="Time Machine not set up" />;
  if (tm.running) {
    const p = tm.progress;
    const pct = p?.fraction != null ? Math.round(p.fraction * 100) : null;
    return (
      <Item
        lead={<Glyph kind="auto" />}
        title="Time Machine is backing up"
        sub={
          <span aria-live="polite">
            {tm.status_text}
            {pct !== null && p?.phase === "copying" ? ` · ${pct}%` : ""}
          </span>
        }
        trail={
          <button className="capsule small" onClick={() => timeMachine("skip")}>
            Skip
          </button>
        }
      >
        <div
          className={`meter thin indent ${pct === null ? "indeterminate" : ""}`}
          role="progressbar"
          aria-label="Backup progress"
          aria-valuenow={pct ?? undefined}
          aria-valuemin={0}
          aria-valuemax={100}
        >
          <div style={{ width: pct === null ? undefined : `${pct}%` }} />
        </div>
        <p className="sub indent caption">
          {tm.remaining_text ?? "Eject is available when the backup is done"}
        </p>
      </Item>
    );
  }
  return (
    <Item
      lead={<Glyph kind="ok" />}
      title="Time Machine idle"
      sub={tm.latest_text ? `Latest backup: ${tm.latest_text}` : "No backups yet"}
      trail={
        <button
          className="capsule small"
          onClick={() => timeMachine("start")}
          disabled={s.backup !== null && !s.backup.mounted}
          title={
            s.backup && !s.backup.mounted ? `The “${s.backup.name}” volume is not mounted` : undefined
          }
        >
          Back Up
        </button>
      }
    />
  );
}

function StoragePlatter({ s }: { s: DetailStatus }) {
  const storage = s.storage;
  const ws = s.workspace.name;
  const bk = s.backup?.name ?? "Backup";
  if (!s.workspace.mounted) return null;
  const st = storage;
  const pct = (n: number) => (st && st.total > 0 ? (n / st.total) * 100 : 0);
  const other = st ? Math.max(0, st.total - st.free - st.workspace - (st.backup ?? 0)) : 0;
  return (
    <section className="platter storage" aria-label="Storage">
      <div className="row">
        <span className="label">Storage</span>
        <span className="secondary right num">
          {st ? `${gb(st.free)} free of ${gb(st.total)}` : "…"}
        </span>
      </div>
      <div
        className="meter stacked"
        role="img"
        aria-label={
          st
            ? `${ws} ${gb(st.workspace)}, ${st.backup != null ? `${bk} ${gb(st.backup)}, ` : ""}${gb(st.free)} free`
            : "Loading"
        }
      >
        {st && (
          <>
            <div className="seg ws" style={{ width: `${pct(st.workspace)}%` }} />
            {st.backup != null && (
              <div className="seg bk" style={{ width: `${pct(st.backup)}%` }} />
            )}
            {other > 1e9 && <div className="seg other" style={{ width: `${pct(other)}%` }} />}
          </>
        )}
      </div>
      {st && (
        <div className="legend num">
          <span>
            <i className="ws" />
            {ws} {gb(st.workspace)}
          </span>
          {st.backup != null && (
            <span>
              <i className="bk" />
              {bk} {gb(st.backup)}
            </span>
          )}
        </div>
      )}
    </section>
  );
}

export default function App() {
  const [status, setStatus] = useState<DetailStatus | null>(null);
  const [holders, setHolders] = useState<Holder[] | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const checkingRef = useRef(false);

  const load = useCallback(() => {
    getStatus()
      .then((s) => {
        setStatus(s);
        // The last known apps, so the verdict shows at once; the fresh check follows.
        if (s.holders) setHolders(s.holders);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  /** lsof: only on open, focus, refresh, and mount/eject changes. */
  const check = useCallback(() => {
    if (checkingRef.current) return;
    checkingRef.current = true;
    setChecking(true);
    listHolders()
      .then(setHolders)
      .catch((e) => setError(String(e)))
      .finally(() => {
        checkingRef.current = false;
        setChecking(false);
      });
  }, []);

  useEffect(() => {
    // The check starts once the status says the SSD is mounted (effect below).
    load();
    const unlisten = onStatusChanged((s) => {
      setStatus(s);
      setError(null);
    });
    const onFocus = () => {
      load();
      check();
    };
    window.addEventListener("focus", onFocus);
    return () => {
      unlisten.then((f) => f());
      window.removeEventListener("focus", onFocus);
    };
  }, [load, check]);

  // The window starts hidden; show it as soon as the first status is in the DOM. A layout
  // effect runs right after the render: requestAnimationFrame (and so useEffect) does not
  // fire in a hidden window.
  const shown = useRef(false);
  useLayoutEffect(() => {
    if (status && !shown.current) {
      shown.current = true;
      detailReady();
    }
  }, [status]);

  const mounted = status?.workspace.mounted ?? false;
  const busy = status?.busy ?? false;
  useEffect(() => {
    if (status && !busy) check();
    if (!mounted) setHolders(null);
  }, [mounted, busy]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!status) {
    return (
      <main className="loading">
        <header className="titlebar" data-tauri-drag-region />
        <p className="secondary">{error ?? "Loading…"}</p>
      </main>
    );
  }

  const v = verdict(status, mounted ? holders : []);
  const backingUp = backingUpToSsd(status);

  return (
    <main>
      <header className="titlebar" data-tauri-drag-region>
        {status.dry_run && (
          <span className="pill" title="Eject is only simulated; no commands are executed">
            Dry run
          </span>
        )}
        <button
          className="icon-button"
          onClick={() => {
            refreshStatus();
            check();
          }}
          aria-label="Refresh status"
          title="Refresh status"
        >
          <RefreshIcon className={checking ? "spin" : ""} />
        </button>
      </header>

      <section className={`hero ${v.tone}`} aria-live="polite" data-tauri-drag-region>
        <span className="hero-badge" aria-hidden>
          <DriveIcon />
        </span>
        <h1>{v.title}</h1>
        <p>{v.sub}</p>
      </section>

      {mounted && (
        <section className="platter grow" aria-label="Before ejecting">
          <ul className="checklist">
            {holders?.map((h) => <HolderItem key={h.bundle ?? h.name} h={h} />)}
            {holders?.length === 0 && (
              <Item lead={<Glyph kind="ok" />} title="No apps using the SSD" />
            )}
            <TimeMachineItem s={status} />
            {status.herd_running ? (
              <Item lead={<Glyph kind="auto" />} title="Herd is running" sub="Will be stopped" />
            ) : (
              <Item lead={<Glyph kind="ok" />} title="Herd not running" />
            )}
          </ul>
        </section>
      )}

      <StoragePlatter s={status} />

      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}

      <footer className="row">
        <button className="capsule" onClick={() => openWorkspace()} disabled={!mounted}>
          <FolderIcon />
          Open in Finder
        </button>
        <button
          className="capsule primary right"
          onClick={() => ejectSsd().catch((e) => setError(String(e)))}
          disabled={!mounted || status.busy || backingUp}
          title={backingUp ? "Available when the Time Machine backup is done" : undefined}
        >
          <EjectIcon />
          {status.busy ? "Ejecting…" : "Eject SSD…"}
        </button>
      </footer>
    </main>
  );
}
