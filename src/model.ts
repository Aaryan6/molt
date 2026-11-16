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
