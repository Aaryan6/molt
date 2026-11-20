import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { deletePaths, diskInfo, scan, type Candidate, type DiskSpace } from "./api";
import FolderList, { type Sort } from "./FolderList";
import {
  ANY_TIME,
  buildItems,
  formatBytes,
  GROUPS,
  IDLE_DAYS,
  isIdle,
  matchesEdited,
  totalFolders,
  totalReclaimable,
  totalShared,
  type EditedFilter,
  type GroupId,
} from "./model";
import Overview from "./Overview";

const ROOT_KEY = "molt.root";
const RECYCLE_KEY = "molt.recycle";

export default function App() {
  const [root, setRoot] = useState<string | null>(() => localStorage.getItem(ROOT_KEY));
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [disk, setDisk] = useState<DiskSpace | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [tab, setTab] = useState<GroupId | "all">("all");
  const [sort, setSort] = useState<Sort>("size");
  const [edited, setEdited] = useState<EditedFilter>(ANY_TIME);
  // Recycle Bin is the default; the choice is remembered.
  const [recycle, setRecycle] = useState(() => localStorage.getItem(RECYCLE_KEY) !== "0");
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<number>(undefined);

  const notify = useCallback((msg: string) => {
    setToast(msg);
    clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 3500);
  }, []);

  const items = useMemo(() => buildItems(candidates), [candidates]);

  const runScan = useCallback(
    async (dir: string) => {
      setCandidates([]);
      setSelected(new Set());
      setScanError(null);
      setScanning(true);
      diskInfo(dir).then(setDisk).catch(() => setDisk(null));
      // Batch streamed results so a big scan does not re-render per folder.
      let buffer: Candidate[] = [];
      const flush = () => {
        if (buffer.length === 0) return;
        const batch = buffer;
        buffer = [];
        setCandidates((prev) => [...prev, ...batch]);
      };
      const timer = window.setInterval(flush, 120);
      try {
        await scan([dir], (c) => buffer.push(c));
      } catch (e) {
        setScanError(String(e));
      } finally {
        clearInterval(timer);
        flush();
        setScanning(false);
      }
    },
    [],
  );

  useEffect(() => {
    if (root) void runScan(root);
    // Scan once on launch with the saved folder.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function pickFolder() {
    const dir = await open({ directory: true, defaultPath: root ?? undefined, title: "Choose a folder to scan" });
    if (typeof dir !== "string") return;
    localStorage.setItem(ROOT_KEY, dir);
    setRoot(dir);
    void runScan(dir);
  }

  const shown = useMemo(() => {
    const group = GROUPS.find((g) => g.id === tab);
    return items
      .filter((i) => (!group || i.group === group) && matchesEdited(i, edited))
      .sort((a, b) =>
        sort === "size"
          ? b.reclaimable - a.reclaimable || b.size - a.size
          : (b.age ?? -1) - (a.age ?? -1),
      );
  }, [items, tab, sort, edited]);

  const chosen = items.filter((i) => selected.has(i.id));
  const chosenFolders = totalFolders(chosen);
  const chosenReclaimable = totalReclaimable(chosen);
  const chosenShared = totalShared(chosen);

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  }

  function toggleAll() {
    const allOn = shown.length > 0 && shown.every((i) => selected.has(i.id));
    setSelected((prev) => {
      const next = new Set(prev);
      shown.forEach((i) => (allOn ? next.delete(i.id) : next.add(i.id)));
      return next;
    });
  }

  function selectIdle() {
    setSelected((prev) => new Set([...prev, ...items.filter(isIdle).map((i) => i.id)]));
    setTab("all");
    setEdited({ kind: "older", days: IDLE_DAYS });
    setSort("age");
  }

  function chooseRecycle(on: boolean) {
    setRecycle(on);
    localStorage.setItem(RECYCLE_KEY, on ? "1" : "0");
  }

  async function confirmDelete() {
    setDeleting(true);
    try {
      const targetPaths = chosen.flatMap((i) => i.paths);
      const results = await deletePaths(targetPaths, recycle);
      const done = new Set(results.filter((r) => !r.error).map((r) => r.path));
      const failed = results.filter((r) => r.error);
      const failedSet = new Set(failed.map((r) => r.path));
      const freed = candidates
        .filter((c) => done.has(c.path))
        .reduce((n, c) => n + Math.max(0, c.size - c.shared_size), 0);
      setCandidates((prev) => prev.filter((c) => !done.has(c.path)));
      setSelected(new Set(chosen.filter((i) => i.paths.some((p) => failedSet.has(p))).map((i) => i.id)));
      if (root) diskInfo(root).then(setDisk).catch(() => {});
      const doneMsg = recycle
        ? `Moved ${formatBytes(freed)} to the Recycle Bin`
        : `Freed ${formatBytes(freed)}`;
      notify(
        failed.length
          ? `${doneMsg}. ${failed.length} couldn't be ${recycle ? "moved" : "deleted"}: ${failed[0].error}`
          : doneMsg,
      );
    } catch (e) {
      notify(`Delete failed: ${e}`);
    } finally {
      setDeleting(false);
      setConfirming(false);
    }
  }

  useEffect(() => {
    if (!confirming) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !deleting) setConfirming(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [confirming, deleting]);

  const drive = root?.match(/^[A-Za-z]:/)?.[0] ?? "";
  const recreate = [...new Set(chosen.map((i) => i.group.recreate))];

  return (
    <div className="app">
      {scanning && (
        <div className="progress">
          <i />
        </div>
      )}

      <header className="top">
        <div className="brand">
          <LogoMark />
          Molt
        </div>
        <button className="chip-btn" onClick={pickFolder} title="Change folder">
          <FolderIcon />
          <span className="path">{root ?? "Choose folder"}</span>
          <ChevronIcon />
        </button>
        <button className="btn primary" disabled={scanning || !root} onClick={() => root && runScan(root)}>
          <RefreshIcon />
          {scanning ? "Scanning…" : "Scan"}
        </button>
      </header>

      <Overview items={items} disk={disk} drive={drive} onSelectIdle={selectIdle} />

      <FolderList
        items={items}
        shown={shown}
        root={root}
        tab={tab}
        sort={sort}
        edited={edited}
        selected={selected}
        scanning={scanning}
        scanError={scanError}
        hasRoot={root !== null}
        onTab={setTab}
        onSort={setSort}
        onEdited={setEdited}
        onToggle={toggle}
        onToggleAll={toggleAll}
        onDelete={() => setConfirming(true)}
        onPickFolder={pickFolder}
      />

      {confirming && (
        <div className="scrim" onClick={(e) => e.target === e.currentTarget && !deleting && setConfirming(false)}>
          <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
            <h2 id="confirm-title">
              Delete {chosenFolders} folder{chosenFolders === 1 ? "" : "s"}?
            </h2>
            <p>
              Your code stays untouched, and you can bring them back with {recreate.slice(0, 2).join(" or ")}
              {recreate.length > 2 ? " and the like" : ""}.
            </p>
            <div className="modes" role="radiogroup" aria-label="How to delete">
              <button role="radio" aria-checked={recycle} className={recycle ? "on" : ""} onClick={() => chooseRecycle(true)} disabled={deleting}>
                Recycle Bin
              </button>
              <button role="radio" aria-checked={!recycle} className={recycle ? "" : "on"} onClick={() => chooseRecycle(false)} disabled={deleting}>
                Delete permanently
              </button>
            </div>
            <div className="free">
              <b className="num">{formatBytes(chosenReclaimable)}</b>
              <span>{recycle ? "freed when you empty the Recycle Bin" : "freed right away, no undo"}</span>
            </div>
            {chosenShared > 0 && (
              <div className="shared-note">
                + {formatBytes(chosenShared)} hardlinked in the pnpm store — run <code>pnpm store prune</code> after to
                free it.
              </div>
            )}
            <div className="actions">
              <button className="btn" disabled={deleting} onClick={() => setConfirming(false)}>
                Cancel
              </button>
              <button className={recycle ? "btn primary" : "btn danger"} disabled={deleting} onClick={confirmDelete} autoFocus>
                {deleting ? (recycle ? "Moving…" : "Deleting…") : recycle ? "Move to Recycle Bin" : "Delete permanently"}
              </button>
            </div>
          </div>
        </div>
      )}

      {toast && (
        <div className="toast">
          <CheckIcon />
          {toast}
        </div>
      )}
    </div>
  );
}

const iconProps = {
  fill: "none",
  stroke: "currentColor",
  strokeLinecap: "round",
  strokeLinejoin: "round",
  viewBox: "0 0 24 24",
} as const;

const FolderIcon = () => (
  <svg width="15" height="15" strokeWidth="1.8" {...iconProps}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
  </svg>
);
const ChevronIcon = () => (
  <svg width="13" height="13" strokeWidth="2" {...iconProps}>
    <path d="m6 9 6 6 6-6" />
  </svg>
);
const RefreshIcon = () => (
  <svg width="15" height="15" strokeWidth="2" {...iconProps}>
    <path d="M20 11a8 8 0 1 0-2.3 5.7M20 5v6h-6" />
  </svg>
);
const CheckIcon = () => (
  <svg width="15" height="15" strokeWidth="2.4" {...iconProps}>
    <path d="m5 12 5 5L20 7" />
  </svg>
);

/** The app icon: a light ring on a dark tile, same proportions as app-icon.png. */
function LogoMark() {
  return (
    <svg className="mark" width="24" height="24" viewBox="0 0 24 24" aria-hidden="true">
      <rect width="24" height="24" rx="6" fill="#18181b" />
      <circle cx="12" cy="12" r="5.25" fill="none" stroke="#f4f4f5" strokeWidth="4.5" />
    </svg>
  );
}
