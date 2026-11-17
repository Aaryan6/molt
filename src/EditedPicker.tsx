import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  AGE_PRESETS,
  ANY_TIME,
  DAY_MS,
  editedDay,
  editedLabel,
  formatBytes,
  matchesEdited,
  startOfDay,
  totalSize,
  type EditedFilter,
  type Item,
} from "./model";

type Props = {
  items: Item[];
  value: EditedFilter;
  onChange: (f: EditedFilter) => void;
};

const WEEKDAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/** First day of the month containing `ms`, at local midnight. */
function monthStart(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), 1).getTime();
}

function addMonths(ms: number, n: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth() + n, 1).getTime();
}

/** 6 weeks of days (Monday first) covering the month that starts at `month`. */
function monthGrid(month: number): number[] {
  const first = new Date(month);
  const offset = (first.getDay() + 6) % 7;
  return Array.from({ length: 42 }, (_, i) =>
    new Date(first.getFullYear(), first.getMonth(), 1 - offset + i).getTime(),
  );
}

/**
 * The "Edited" filter: a button that opens a popover with quick presets and
 * a calendar for picking a range of last-edited days.
 */
export default function EditedPicker({ items, value, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState({ top: 0, right: 0 });
  const [month, setMonth] = useState(() => monthStart(Date.now()));
  // First click of a range, while waiting for the second.
  const [anchor, setAnchor] = useState<number | null>(null);
  const [hover, setHover] = useState<number | null>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);

  const today = startOfDay(Date.now());
  const thisMonth = monthStart(today);

  // How many folders were last edited on each day, for the activity dots.
  const activity = useMemo(() => {
    const m = new Map<number, number>();
    for (const i of items) {
      const d = editedDay(i);
      if (d !== null) m.set(d, (m.get(d) ?? 0) + 1);
    }
    return m;
  }, [items]);

  function show() {
    const r = btn.current!.getBoundingClientRect();
    setPos({ top: r.bottom + 6, right: window.innerWidth - r.right });
    setAnchor(null);
    setHover(null);
    // Open on the month where the current filter ends, so its band is visible.
    const focus =
      value.kind === "range" ? value.to : value.kind === "older" ? today - value.days * DAY_MS : today;
    setMonth(monthStart(focus));
    setOpen(true);
  }

  // Keep the popover inside the window when the list sits low.
  useLayoutEffect(() => {
    if (!open || !pop.current || !btn.current) return;
    const h = pop.current.offsetHeight;
    const r = btn.current.getBoundingClientRect();
    if (r.bottom + 6 + h > window.innerHeight - 8) {
      setPos((p) => ({ ...p, top: Math.max(8, r.top - 6 - h) }));
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (!pop.current?.contains(t) && !btn.current?.contains(t)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        btn.current?.focus();
      }
    };
    const onResize = () => setOpen(false);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onResize);
    };
  }, [open]);

  function pickDay(day: number) {
    if (anchor === null) {
      setAnchor(day);
      onChange({ kind: "range", from: day, to: day });
    } else {
      onChange({ kind: "range", from: Math.min(anchor, day), to: Math.max(anchor, day) });
      setAnchor(null);
    }
  }

  function pickPreset(f: EditedFilter) {
    setAnchor(null);
    onChange(f);
    setOpen(false);
  }

  // The highlighted band: a live preview while picking, else the current filter.
  let band: { from: number; to: number } | null = null;
  if (anchor !== null) {
    const other = hover ?? anchor;
    band = { from: Math.min(anchor, other), to: Math.max(anchor, other) };
  } else if (value.kind === "range") {
    band = { from: value.from, to: value.to };
  } else if (value.kind === "older") {
    band = { from: -Infinity, to: today - value.days * DAY_MS };
  }

  const matching = items.filter((i) => matchesEdited(i, value));
  const count = (f: EditedFilter) => items.filter((i) => matchesEdited(i, f)).length;
  const monthName = new Date(month).toLocaleDateString("en-GB", { month: "long", year: "numeric" });
  const on = value.kind !== "any";

  return (
    <>
      <button
        ref={btn}
        className={on ? "filter on" : "filter"}
        onClick={() => (open ? setOpen(false) : show())}
        aria-haspopup="dialog"
        aria-expanded={open}
        title="Filter by when the project was last edited"
      >
        <ClockIcon />
        <span>{editedLabel(value)}</span>
        <ChevronIcon />
      </button>

      {open && (
        <div ref={pop} className="pop" role="dialog" aria-label="Filter by last edited" style={pos}>
          <div className="presets">
            <div className="pop-label">Last edited</div>
            <Preset label="Any time" n={items.length} on={value.kind === "any"} onClick={() => pickPreset(ANY_TIME)} />
            <div className="pop-label sub">Older than</div>
            {AGE_PRESETS.map((p) => {
              const f: EditedFilter = { kind: "older", days: p.days };
              return (
                <Preset
                  key={p.days}
                  label={p.short}
                  title={p.label}
                  n={count(f)}
                  on={value.kind === "older" && value.days === p.days}
                  onClick={() => pickPreset(f)}
                />
              );
            })}
          </div>

          <div className="cal">
            <div className="cal-head">
              <button className="icon-btn" onClick={() => setMonth(addMonths(month, -12))} aria-label="Previous year">
                <DoubleChevron dir="left" />
              </button>
              <button className="icon-btn" onClick={() => setMonth(addMonths(month, -1))} aria-label="Previous month">
                <Chevron dir="left" />
              </button>
              <b>{monthName}</b>
              <button
                className="icon-btn"
                onClick={() => setMonth(addMonths(month, 1))}
                disabled={month >= thisMonth}
                aria-label="Next month"
              >
                <Chevron dir="right" />
              </button>
              <button
                className="icon-btn"
                onClick={() => setMonth(Math.min(thisMonth, addMonths(month, 12)))}
                disabled={month >= thisMonth}
                aria-label="Next year"
              >
                <DoubleChevron dir="right" />
              </button>
            </div>

            <div className="cal-grid" onMouseLeave={() => setHover(null)}>
              {WEEKDAYS.map((w) => (
                <span key={w} className="wd">
                  {w}
                </span>
              ))}
              {monthGrid(month).map((day) => {
                const d = new Date(day);
                const inMonth = d.getMonth() === new Date(month).getMonth();
                const future = day > today;
                const inBand = band !== null && day >= band.from && day <= band.to;
                const cls = [
                  "day",
                  inMonth ? "" : "out",
                  day === today ? "today" : "",
                  inBand ? "in" : "",
                  inBand && day === band!.from ? "start" : "",
                  inBand && day === band!.to ? "end" : "",
                ]
                  .filter(Boolean)
                  .join(" ");
                const n = activity.get(day) ?? 0;
                return (
                  <button
                    key={day}
                    className={cls}
                    disabled={future}
                    onClick={() => pickDay(day)}
                    onMouseEnter={() => setHover(day)}
                    aria-label={`${d.toDateString()}${n ? `, ${n} folder${n === 1 ? "" : "s"} last edited` : ""}`}
                    aria-pressed={inBand}
                  >
                    <span>{d.getDate()}</span>
                    {n > 0 && <i className="act" />}
                  </button>
                );
              })}
            </div>
            <div className="cal-hint">
              {anchor !== null ? "Pick the end day" : "Pick a day, or two days for a range"}
            </div>
          </div>

          <div className="pop-foot">
            <span className="info">
              <b>{matching.length}</b> folder{matching.length === 1 ? "" : "s"} ·{" "}
              <b className="num">{formatBytes(totalSize(matching))}</b>
            </span>
            {on && (
              <button className="btn sm ghost" onClick={() => pickPreset(ANY_TIME)}>
                Clear
              </button>
            )}
            <button className="btn sm primary" onClick={() => setOpen(false)}>
              Done
            </button>
          </div>
        </div>
      )}
    </>
  );
}

type PresetProps = { label: string; title?: string; n: number; on: boolean; onClick: () => void };

function Preset({ label, title, n, on, onClick }: PresetProps) {
  return (
    <button className={on ? "preset on" : "preset"} onClick={onClick} title={title}>
      <span className="lbl">{label}</span>
      <span className="n">{n}</span>
    </button>
  );
}

const svg = {
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

function ClockIcon() {
  return (
    <svg width="14" height="14" strokeWidth="2" {...svg}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </svg>
  );
}

function ChevronIcon() {
  return (
    <svg className="chev" width="12" height="12" strokeWidth="2.2" {...svg}>
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function Chevron({ dir }: { dir: "left" | "right" }) {
  return (
    <svg width="14" height="14" strokeWidth="2.2" {...svg}>
      <path d={dir === "left" ? "m15 6-6 6 6 6" : "m9 6 6 6-6 6"} />
    </svg>
  );
}

function DoubleChevron({ dir }: { dir: "left" | "right" }) {
  return (
    <svg width="14" height="14" strokeWidth="2.2" {...svg}>
      <path d={dir === "left" ? "m12 6-6 6 6 6M19 6l-6 6 6 6" : "m5 6 6 6-6 6M12 6l6 6-6 6"} />
    </svg>
  );
}
