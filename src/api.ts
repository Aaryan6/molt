import { Channel, invoke } from "@tauri-apps/api/core";

export type Kind =
  | "node-modules"
  | "next"
  | "nuxt"
  | "turbo"
  | "dist"
  | "build"
  | "out"
  | "python-venv"
  | "py-cache"
  | "rust-target"
  | "svelte-kit"
  | "angular"
  | "astro"
  | "nuxt-output"
  | "parcel-cache"
  | "expo"
  | "coverage"
  | "py-tool-cache"
  | "tox"
  | "maven-target"
  | "gradle"
  | "dotnet-build"
  | "dart"
  | "zig"
  | "vendor"
  | "cache";

export type Candidate = {
  path: string;
  /** Owning project directory (enclosing git repo or manifest root, else parent). */
  project_root: string;
  kind: Kind;
  size: number;
  shared_size: number;
  /** Seconds since epoch of the owning project's last activity. */
  last_active: number | null;
};

export type DiskSpace = { total: number; free: number };
export type DeleteResult = { path: string; error: string | null };

export function scan(roots: string[], onFound: (c: Candidate) => void) {
  const channel = new Channel<Candidate>();
  channel.onmessage = onFound;
  return invoke<void>("scan", { roots, onFound: channel });
}

/** Delete folders permanently, or move them to the Recycle Bin. */
export const deletePaths = (paths: string[], recycle: boolean) =>
  invoke<DeleteResult[]>("delete_paths", { paths, recycle });

export const diskInfo = (path: string) => invoke<DiskSpace | null>("disk_info", { path });
