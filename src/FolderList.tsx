import { useEffect, useRef } from "react";
import EditedPicker from "./EditedPicker";
import {
  ANY_TIME,
  bulkPickable,
  formatAge,
  formatBytes,
  GROUPS,
  isIdle,
  isRecent,
  matchesEdited,
  splitDisplayPath,
  totalFolders,
  totalReclaimable,
  totalShared,
  type EditedFilter,
  type GroupId,
  type Item,
} from "./model";

export type Sort = "size" | "age";

type Props = {
  items: Item[];
  shown: Item[];
  root: string | null;
  tab: GroupId | "all";
  sort: Sort;
  /** Filter on when each project was last edited. */
  edited: EditedFilter;
  selected: Set<string>;
  scanning: boolean;
  scanError: string | null;
  hasRoot: boolean;
  onTab: (tab: GroupId | "all") => void;
  onSort: (sort: Sort) => void;
  onEdited: (f: EditedFilter) => void;
  onToggle: (id: string) => void;
  onToggleAll: () => void;
  onDelete: () => void;
  onPickFolder: () => void;
};

export default function FolderList(p: Props) {
  // Tab counts follow the age filter, so they match what each tab shows.
  const pool = p.items.filter((i) => matchesEdited(i, p.edited));
  const tabs = [
    { id: "all" as const, label: "All", count: pool.length, color: undefined },
    ...GROUPS.map((g) => ({
      id: g.id,
      label: g.label,
      count: pool.filter((i) => i.group === g).length,
      color: g.color,
    })),
  ].filter((t) => t.id === "all" || t.count > 0 || t.id === p.tab);

  const max = Math.max(1, ...p.items.map((i) => i.reclaimable));
  const nSel = p.shown.filter((i) => p.selected.has(i.id)).length;
  const pickable = bulkPickable(p.shown, p.edited);
  const allOn = pickable.length > 0 && pickable.every((i) => p.selected.has(i.id));
  const leftRecent = p.edited.kind === "any" ? p.shown.filter((i) => isRecent(i) && !p.selected.has(i.id)).length : 0;
  const chosen = p.items.filter((i) => p.selected.has(i.id));
  const hiddenSel = chosen.length - nSel;
  const chosenFolders = totalFolders(chosen);
  const chosenReclaimable = totalReclaimable(chosen);
  const chosenShared = totalShared(chosen);

  const allRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (allRef.current) allRef.current.indeterminate = nSel > 0 && !allOn;
  }, [nSel, allOn]);

  return (
    <section className="list-card">
      <div className="list-top">
        <div className="tabs">
          {tabs.map((t) => (
            <button key={t.id} className={p.tab === t.id ? "tab on" : "tab"} onClick={() => p.onTab(t.id)}>
              {t.color && <i className="dot" style={{ background: t.color }} />}
              {t.label}
              <span className="n">{t.count}</span>
            </button>
          ))}
        </div>
        <div className="tools">
        <EditedPicker items={p.items} value={p.edited} onChange={p.onEdited} />
        <div className="sort">
          {(["size", "age"] as const).map((s) => (
            <button key={s} className={p.sort === s ? "on" : ""} onClick={() => p.onSort(s)}>
              {s === "size" ? "Largest" : "Oldest"}
            </button>
          ))}
        </div>
        </div>
      </div>

      <div className="cols">
        <input
          ref={allRef}
          type="checkbox"
          aria-label="Select all"
          title="Selects everything shown except projects edited this week"
          checked={allOn}
          disabled={pickable.length === 0}
          onChange={p.onToggleAll}
        />
        <span>Folder</span>
        <span>Type</span>
        <span>Share of total</span>
        <span>Edited</span>
        <span className="r">Size</span>
      </div>

      <div className="rows">
        {p.shown.map((i) => {
          const on = p.selected.has(i.id);
          const { prefix, leaf } = splitDisplayPath(i.path, p.root);
          return (
            <div key={i.id} className={on ? "row sel" : "row"} onClick={() => p.onToggle(i.id)}>
              <input type="checkbox" tabIndex={-1} checked={on} readOnly />
              <div className="name">
                <b>{i.project}</b>
                <span className="path-line mono" title={i.path}>
                  {prefix && <span className="dir">{prefix}</span>}
                  <span className="leaf">{leaf}</span>
                </span>
              </div>
              <div>
                <span className="kind" title={i.paths.length > 1 ? `${i.paths.length} __pycache__ folders` : i.folder}>
                  <i className="dot" style={{ background: i.group.color }} />
                  {i.folder}
                </span>
              </div>
              <div className="meter">
                <i style={{ width: `${(i.reclaimable / max) * 100}%`, background: i.group.color }} />
              </div>
              <span className={isIdle(i) ? "age old" : "age"}>{formatAge(i.age)}</span>
              <span
                className="size num r"
                title={
                  i.shared > 0
                    ? `${formatBytes(i.size)} total (${formatBytes(i.shared)} hardlinked in pnpm store — run pnpm store prune after deleting to free it)`
                    : undefined
                }
              >
                <b>{formatBytes(i.reclaimable)}</b>
                {i.shared > 0 && <small>{formatBytes(i.size)} pnpm</small>}
              </span>
            </div>
          );
        })}
        {p.shown.length === 0 && (
          <div className="empty">
            {p.scanning ? (
              "Looking for folders…"
            ) : p.scanError ? (
              <>
                <div className="err">{p.scanError}</div>
                <button className="btn" onClick={p.onPickFolder}>
                  Choose another folder
                </button>
              </>
            ) : !p.hasRoot ? (
              <>
                <div>Pick the folder where you keep your projects.</div>
                <button className="btn" onClick={p.onPickFolder}>
                  Choose folder
                </button>
              </>
            ) : p.items.length > 0 ? (
              <>
                <div>No folders match these filters.</div>
                <button
                  className="btn"
                  onClick={() => {
                    p.onEdited(ANY_TIME);
                    p.onTab("all");
                  }}
                >
                  Show everything
                </button>
              </>
            ) : (
              "Nothing to clean here. Nice."
            )}
          </div>
        )}
      </div>

      <div className="list-foot">
        <span className="info">
          {chosen.length ? (
            <>
              <b>{chosenFolders}</b> {chosenFolders === 1 ? "folder" : "folders"} selected ·{" "}
              <b className="num">{formatBytes(chosenReclaimable)}</b> to free
              {chosenShared > 0 && <span className="hidden-note"> · +{formatBytes(chosenShared)} pnpm</span>}
              {hiddenSel > 0 && <span className="hidden-note"> · {hiddenSel} hidden by filters</span>}
              {leftRecent > 0 && (
                <span className="hidden-note" title="Projects edited this week are never ticked in bulk">
                  {" "}
                  · {leftRecent} edited this week not ticked
                </span>
              )}
            </>
          ) : (
            "Pick folders to free up space"
          )}
        </span>
        <button className="btn danger" disabled={chosen.length === 0} onClick={p.onDelete}>
          <TrashIcon />
          Delete
        </button>
      </div>
    </section>
  );
}

function TrashIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3" />
    </svg>
  );
}
