# Molt

Shed the folders that grow back.

Molt is a Windows app that finds the folders your dev projects can regenerate, such as `node_modules`, `.next` and Rust `target`, shows how much space they take, and deletes the ones you pick.

## Download

Get `Molt.exe` from the [latest release](https://github.com/Aaryan6/molt/releases/latest). It's a portable app with no installer. It runs on Windows 10 and 11 and uses WebView2, which Windows 11 already includes. The exe isn't code-signed, so SmartScreen may warn you the first time: click **More info**, then **Run anyway**.

The same release has `molt-cli.exe`, the command-line version.

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

Molt only looks at your own projects. It skips symlinks, junctions, hidden dot-folders such as `.vscode`, `.cache` and `.cursor` (but still finds a project's `.next` or `.venv`), and system folders such as `AppData`, `Program Files` and `Windows`. It also skips pnpm's store and global packages, wherever they live, and Yarn's global packages. If you pick a folder inside any of these, Molt tells you instead of listing the apps and tools installed there, and it won't delete anything inside them.

Nothing is deleted until you confirm. In the confirm dialog you choose whether folders go to the Recycle Bin, which is the default and can be undone, or are deleted permanently. Recycled folders still take up space until you empty the Recycle Bin. Before deleting or recycling a folder, Molt checks the rules again.

Use the **Edited** filter to see only projects you haven't touched in a week, a month, 3 months, 6 months or a year. To clear out everything older than a month, pick "Older than 1 month", tick the header checkbox and press Delete.

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
cargo build --release -p molt   # builds target/release/molt.exe
molt scan D:\code --min-mb 100
molt clean D:\code --older-than-days 30          # dry run
molt clean D:\code --older-than-days 30 --yes    # delete
molt clean D:\code --older-than-days 30 --yes --recycle   # to the Recycle Bin
```

## Layout

- `crates/core`: the scanner, the folder rules and deletion
- `crates/cli`: the `molt` command
- `src-tauri`: the Tauri 2 desktop app
- `src`: the React frontend
- `design/molt-ui.html`: the UI mockup

## License

MIT. See [LICENSE](LICENSE).
