import { useEffect, useRef } from "react";
import { formatAge, formatBytes, GROUPS, isIdle, totalSize, type GroupId, type Item } from "./model";

export type Sort = "size" | "age";

type Props = {
  items: Item[];
  shown: Item[];
  tab: GroupId | "all";
  sort: Sort;
  selected: Set<string>;
  scanning: boolean;
  hasRoot: boolean;
  onTab: (tab: GroupId | "all") => void;
  onSort: (sort: Sort) => void;
  onToggle: (path: string) => void;
  onToggleAll: () => void;
  onDelete: () => void;
  onPickFolder: () => void;
};

export default function FolderList(p: Props) {
  const tabs = [
    { id: "all" as const, label: "All", count: p.items.length, color: undefined },
    ...GROUPS.map((g) => ({
      id: g.id,
      label: g.label,
      count: p.items.filter((i) => i.group === g).length,
      color: g.color,
    })),
  ].filter((t) => t.id === "all" || t.count > 0);

  const max = Math.max(1, ...p.items.map((i) => i.size));
  const nSel = p.shown.filter((i) => p.selected.has(i.path)).length;
  const chosen = p.items.filter((i) => p.selected.has(i.path));

  const allRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (allRef.current) allRef.current.indeterminate = nSel > 0 && nSel < p.shown.length;
  }, [nSel, p.shown.length]);

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
        <div className="sort">
          {(["size", "age"] as const).map((s) => (
            <button key={s} className={p.sort === s ? "on" : ""} onClick={() => p.onSort(s)}>
              {s === "size" ? "Largest" : "Oldest"}
            </button>
          ))}
        </div>
      </div>

      <div className="cols">
        <input
          ref={allRef}
          type="checkbox"
          aria-label="Select all"
          checked={p.shown.length > 0 && nSel === p.shown.length}
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
          const on = p.selected.has(i.path);
          return (
            <div key={i.path} className={on ? "row sel" : "row"} onClick={() => p.onToggle(i.path)}>
              <input type="checkbox" tabIndex={-1} checked={on} readOnly />
              <div className="name">
                <b>{i.project}</b>
                <span className="mono" title={i.path}>
                  {i.path}
                </span>
              </div>
              <div>
                <span className="kind">
                  <i className="dot" style={{ background: i.group.color }} />
                  {i.folder}
                </span>
              </div>
              <div className="meter">
                <i style={{ width: `${(i.size / max) * 100}%`, background: i.group.color }} />
              </div>
              <span className={isIdle(i) ? "age old" : "age"}>{formatAge(i.age)}</span>
              <span className="size num r">{formatBytes(i.size)}</span>
            </div>
          );
        })}
        {p.shown.length === 0 && (
          <div className="empty">
            {p.scanning ? (
              "Looking for folders…"
            ) : !p.hasRoot ? (
              <>
                <div>Pick the folder where you keep your projects.</div>
                <button className="btn" onClick={p.onPickFolder}>
                  Choose folder
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
              <b>{chosen.length}</b> selected · <b className="num">{formatBytes(totalSize(chosen))}</b> to free
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
