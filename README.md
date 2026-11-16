# Molt

Shed the folders that grow back.

Molt is a Windows app that finds the folders your dev projects can regenerate, such as `node_modules`, `.next` and Rust `target`, shows how much space they take, and deletes the ones you pick.

## What it finds

| Folder | Found when | Brought back by |
| --- | --- | --- |
| `node_modules` | a `package.json` sits next to it | `npm install` |
| `.next`, `.nuxt`, `.turbo` | a `package.json` sits next to it | a rebuild |
| `dist`, `build` | a `package.json` sits next to it and `.gitignore` lists the folder | a rebuild |
| `out` | a Next.js project (no `remotion.config.*`) whose `.gitignore` lists it | a rebuild |
| `target` | a `Cargo.toml` sits next to it | `cargo build` |
| Python venvs | the folder has a `pyvenv.cfg` | `pip install` |
| `__pycache__` | always | running the code |

Molt skips symlinks, junctions, `.git`, and system folders such as `AppData`, `Program Files` and `Windows`.

Deletion is permanent and only happens after you confirm it. Before deleting a folder, Molt checks the rules again.

For pnpm projects, the size shown is the full folder size. Most of those files are hardlinks to the pnpm store, so deleting frees less space until you run `pnpm store prune`.

## Build

You need Rust, Node.js and pnpm.

```sh
pnpm install
pnpm build
cargo build --release -p molt-app --features molt-app/custom-protocol
```

The app is built to `target/release/molt-app.exe`.

## CLI

```sh
cargo build --release -p molt
molt scan D:\code --min-mb 100
molt clean D:\code --older-than-days 30          # dry run
molt clean D:\code --older-than-days 30 --yes    # delete
```

## Layout

- `crates/core`: the scanner, the folder rules and deletion
- `crates/cli`: the `molt` command
- `src-tauri`: the Tauri 2 desktop app
- `src`: the React frontend
- `design/molt-ui.html`: the UI mockup
