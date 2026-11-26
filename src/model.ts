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
  { id: "deps", label: "Dependencies", kinds: ["node-modules", "vendor"], color: "var(--k-deps)", recreate: "npm install" },
  { id: "next", label: ".next", kinds: ["next"], color: "var(--k-next)", recreate: "a rebuild" },
  { id: "rust", label: "Rust target", kinds: ["rust-target"], color: "var(--k-rust)", recreate: "cargo build" },
  {
    id: "py",
    label: "Python",
    kinds: ["python-venv", "py-cache", "py-tool-cache", "tox"],
    color: "var(--k-py)",
    recreate: "pip install",
  },
  {
    id: "build",
    label: "Builds",
    kinds: [
      "dist",
      "build",
      "out",
      "turbo",
      "nuxt",
      "nuxt-output",
      "svelte-kit",
      "angular",
      "astro",
      "parcel-cache",
      "expo",
      "coverage",
      "maven-target",
      "gradle",
      "dotnet-build",
      "dart",
      "zig",
      "cache",
    ],
    color: "var(--k-build)",
    recreate: "a rebuild",
  },
];

/** How to get a folder of each kind back, where it differs from its group's. */
const RECREATE: Partial<Record<Kind, string>> = {
  vendor: "composer install",
  "py-cache": "running the code",
  "py-tool-cache": "running your tools",
  tox: "tox",
  coverage: "running your tests",
  "maven-target": "mvn package",
  gradle: "gradle build",
  "dotnet-build": "dotnet build",
  dart: "flutter pub get",
  zig: "zig build",
  cache: "the tool that made them",
};

export const recreateFor = (kind: Kind) => RECREATE[kind] ?? groupFor(kind).recreate;

export type Item = {
  /** Stable row key (folder path, or `pycache:<project_root>` when grouped). */
  id: string;
  /** Exact folder paths on disk deleted when this row is deleted. */
  paths: string[];
  /** Path shown for this row (the folder itself, or `<project_root>\**\__pycache__` when grouped). */
  path: string;
  /** Owning project directory. */
  projectRoot: string;
  kind: Kind;
  /** Logical size in bytes. */
  size: number;
  /** Bytes hardlinked elsewhere (pnpm store). */
  shared: number;
  /** Bytes freed immediately on delete (`size - shared`). */
  reclaimable: number;
  /** Seconds since epoch of the owning project's last activity. */
  last_active: number | null;
  /** Name of the owning project (with workspace subpath if nested). */
  project: string;
  /** The folder's own name, e.g. `node_modules`, `.venv`, or `__pycache__ ×12`. */
  folder: string;
  group: Group;
  /** Days since the project was last edited. */
  age: number | null;
};

export const IDLE_DAYS = 30;

function splitParts(p: string): string[] {
  return p.split(/[\\/]/).filter(Boolean);
}

function daysSince(epochSecs: number | null): number | null {
  return epochSecs === null ? null : Math.max(0, Math.floor((Date.now() / 1000 - epochSecs) / 86400));
}

function groupFor(kind: Kind): Group {
  return GROUPS.find((g) => g.kinds.includes(kind)) ?? GROUPS[GROUPS.length - 1];
}

function projectLabel(projectRoot: string, folderPath: string, grouped: boolean): string {
  const rootParts = splitParts(projectRoot);
  const rootName = rootParts[rootParts.length - 1] ?? projectRoot;
  if (grouped) return rootName;
  const folderParts = splitParts(folderPath);
  // Show nested workspace subpath (e.g. `noah-2-fe / packages/api-client`) when
  // the target sits deeper inside a monorepo.
  const relParent = folderParts.slice(rootParts.length, -1);
  return relParent.length > 0 ? `${rootName} / ${relParent.join("/")}` : rootName;
}

/**
 * Convert raw scanner candidates into UI rows. Multiple `__pycache__` folders
 * belonging to the same project are merged into one row so Python repos don't
 * flood the list with dozens of tiny subpackage caches.
 */
export function buildItems(candidates: Candidate[]): Item[] {
  const items: Item[] = [];
  const pyGroups = new Map<string, Candidate[]>();

  for (const c of candidates) {
    if (c.kind === "py-cache") {
      const key = c.project_root.toLowerCase();
      const list = pyGroups.get(key);
      if (list) list.push(c);
      else pyGroups.set(key, [c]);
      continue;
    }
    const parts = splitParts(c.path);
    const folder = parts[parts.length - 1] ?? c.path;
    const reclaimable = Math.max(0, c.size - c.shared_size);
    items.push({
      id: c.path,
      paths: [c.path],
      path: c.path,
      projectRoot: c.project_root,
      kind: c.kind,
      size: c.size,
      shared: c.shared_size,
      reclaimable,
      last_active: c.last_active,
      project: projectLabel(c.project_root, c.path, false),
      folder,
      group: groupFor(c.kind),
      age: daysSince(c.last_active),
    });
  }

  for (const [key, list] of pyGroups) {
    const projectRoot = list[0].project_root;
    const size = list.reduce((n, c) => n + c.size, 0);
    const shared = list.reduce((n, c) => n + c.shared_size, 0);
    const reclaimable = Math.max(0, size - shared);
    let lastActive: number | null = null;
    for (const c of list) {
      if (c.last_active !== null && (lastActive === null || c.last_active > lastActive)) {
        lastActive = c.last_active;
      }
    }
    const grouped = list.length > 1;
    const sep = projectRoot.includes("/") && !projectRoot.includes("\\") ? "/" : "\\";
    const displayPath = grouped ? `${projectRoot}${sep}**${sep}__pycache__` : list[0].path;
    items.push({
      id: `pycache:${key}`,
      paths: list.map((c) => c.path),
      path: displayPath,
      projectRoot,
      kind: "py-cache",
      size,
      shared,
      reclaimable,
      last_active: lastActive,
      project: projectLabel(projectRoot, list[0].path, grouped),
      folder: grouped ? `__pycache__ ×${list.length}` : "__pycache__",
      group: groupFor("py-cache"),
      age: daysSince(lastActive),
    });
  }

  return items;
}

/**
 * Split a row's display path into a truncatable prefix (relative to the scanned
 * root when possible) and an always-visible target leaf (e.g. `\node_modules`
 * or `\**\__pycache__`), so long paths never hide what will be deleted.
 */
export function splitDisplayPath(fullPath: string, root: string | null): { prefix: string; leaf: string } {
  let shown = fullPath;
  if (root) {
    const normRoot = root.replace(/[\\/]+$/, "");
    if (
      fullPath.length > normRoot.length &&
      fullPath.slice(0, normRoot.length).toLowerCase() === normRoot.toLowerCase() &&
      (fullPath[normRoot.length] === "\\" || fullPath[normRoot.length] === "/")
    ) {
      shown = fullPath.slice(normRoot.length + 1);
    }
  }
  const globIdx = shown.lastIndexOf("\\**\\");
  if (globIdx >= 0) {
    return { prefix: shown.slice(0, globIdx), leaf: shown.slice(globIdx) };
  }
  const globSlashIdx = shown.lastIndexOf("/**/");
  if (globSlashIdx >= 0) {
    return { prefix: shown.slice(0, globSlashIdx), leaf: shown.slice(globSlashIdx) };
  }
  const sepIdx = Math.max(shown.lastIndexOf("\\"), shown.lastIndexOf("/"));
  if (sepIdx > 0) {
    return { prefix: shown.slice(0, sepIdx), leaf: shown.slice(sepIdx) };
  }
  return { prefix: "", leaf: shown };
}

export const isIdle = (i: Item) => i.age !== null && i.age >= IDLE_DAYS;

export const RECENT_DAYS = 7;

/** Edited in the last week, or Molt couldn't tell when. */
export const isRecent = (i: Item) => i.age === null || i.age < RECENT_DAYS;

/**
 * Rows the header checkbox ticks. With no Edited filter, folders in projects
 * edited this week are left out, so a project being worked on is never
 * cleaned by accident; tick them one by one, or filter by date first.
 */
export const bulkPickable = (shown: Item[], f: EditedFilter) =>
  f.kind === "any" ? shown.filter((i) => !isRecent(i)) : shown;

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
export const totalReclaimable = (items: { reclaimable: number }[]) =>
  items.reduce((n, i) => n + i.reclaimable, 0);
export const totalShared = (items: { shared: number }[]) => items.reduce((n, i) => n + i.shared, 0);
export const totalFolders = (items: { paths: string[] }[]) => items.reduce((n, i) => n + i.paths.length, 0);

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
