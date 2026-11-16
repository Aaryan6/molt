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
  | "rust-target";

export type Candidate = {
  path: string;
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

export const deletePaths = (paths: string[]) =>
  invoke<DeleteResult[]>("delete_paths", { paths });

export const diskInfo = (path: string) => invoke<DiskSpace | null>("disk_info", { path });
