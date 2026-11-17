import type { Candidate, Kind } from "./api";

export type GroupId = "deps" | "next" | "rust" | "py" | "build";

export type Group = {
  id: GroupId;
  label: string;
  kinds: Kind[];
  color: string;
  /** How the user gets these folders back, for the confirm dialog. */
  recreate: string;
};

export const GROUPS: Group[] = [
  { id: "deps", label: "node_modules", kinds: ["node-modules"], color: "var(--k-deps)", recreate: "npm install" },
  { id: "next", label: ".next", kinds: ["next"], color: "var(--k-next)", recreate: "a rebuild" },
  { id: "rust", label: "Rust target", kinds: ["rust-target"], color: "var(--k-rust)", recreate: "cargo build" },
  { id: "py", label: "Python", kinds: ["python-venv", "py-cache"], color: "var(--k-py)", recreate: "pip install" },
  {
    id: "build",
    label: "Builds",
    kinds: ["dist", "build", "out", "turbo", "nuxt"],
    color: "var(--k-build)",
    recreate: "a rebuild",
  },
];

export type Item = Candidate & {
  /** Name of the folder that owns this one, e.g. the project folder. */
  project: string;
  /** The folder's own name, e.g. `node_modules` or `.venv`. */
  folder: string;
  group: Group;
  /** Days since the project was last edited. */
  age: number | null;
};

export const IDLE_DAYS = 30;

export function toItem(c: Candidate): Item {
  const parts = c.path.split(/[\\/]/).filter(Boolean);
  return {
    ...c,
    folder: parts[parts.length - 1] ?? c.path,
    project: parts[parts.length - 2] ?? c.path,
    group: GROUPS.find((g) => g.kinds.includes(c.kind)) ?? GROUPS[GROUPS.length - 1],
    age: c.last_active === null ? null : Math.max(0, Math.floor((Date.now() / 1000 - c.last_active) / 86400)),
  };
}

export const isIdle = (i: Item) => i.age !== null && i.age >= IDLE_DAYS;

/**
 * "Edited" filter on when a project was last touched: anything, untouched
 * for at least `days`, or last edited between two local days (inclusive,
 * stored as the ms timestamp of each day's local midnight).
 */
export type EditedFilter =
  | { kind: "any" }
  | { kind: "older"; days: number }
  | { kind: "range"; from: number; to: number };

export const ANY_TIME: EditedFilter = { kind: "any" };

export const AGE_PRESETS = [
  { days: 7, short: "1 week", label: "Older than 1 week" },
  { days: 30, short: "1 month", label: "Older than 1 month" },
  { days: 90, short: "3 months", label: "Older than 3 months" },
  { days: 180, short: "6 months", label: "Older than 6 months" },
  { days: 365, short: "1 year", label: "Older than 1 year" },
] as const;

export const DAY_MS = 86_400_000;

/** Local midnight of the day containing `ms`. */
export function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** Local midnight of the day containing the item's last activity. */
export const editedDay = (i: Item) => (i.last_active === null ? null : startOfDay(i.last_active * 1000));

export function matchesEdited(i: Item, f: EditedFilter): boolean {
  if (f.kind === "any") return true;
  if (f.kind === "older") return i.age !== null && i.age >= f.days;
  const day = editedDay(i);
  return day !== null && day >= f.from && day <= f.to;
}

function shortDate(ms: number, withYear: boolean): string {
  return new Date(ms).toLocaleDateString("en-GB", {
    day: "numeric",
    month: "short",
    ...(withYear ? { year: "numeric" } : {}),
  });
}

/** Short label for the filter button, e.g. "3 Mar – 21 Jun 2026". */
export function editedLabel(f: EditedFilter): string {
  if (f.kind === "any") return "Edited: any time";
  if (f.kind === "older") return AGE_PRESETS.find((p) => p.days === f.days)?.label ?? `Older than ${f.days} days`;
  const thisYear = new Date().getFullYear();
  const [a, b] = [new Date(f.from), new Date(f.to)];
  if (f.from === f.to) return `Edited ${shortDate(f.from, a.getFullYear() !== thisYear)}`;
  const sameYear = a.getFullYear() === b.getFullYear();
  return `${shortDate(f.from, !sameYear)} – ${shortDate(f.to, !sameYear || b.getFullYear() !== thisYear)}`;
}
export const totalSize = (items: { size: number }[]) => items.reduce((n, i) => n + i.size, 0);

/** Number and unit separately, so the unit can be styled smaller. */
export function splitBytes(bytes: number): [string, string] {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return [i === 0 || v >= 100 ? v.toFixed(0) : v.toFixed(1), units[i]];
}

export const formatBytes = (bytes: number) => splitBytes(bytes).join(" ");

export function formatAge(days: number | null): string {
  if (days === null) return "—";
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 30) return `${days} days`;
  if (days < 365) return `${Math.floor(days / 30)} mo`;
  return `${Math.floor(days / 365)} yr`;
}
