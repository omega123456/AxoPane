# AxoPane

A fast, lightweight **dual-pane file explorer** for **Windows and macOS**, inspired by xplorer2. AxoPane pairs two independent file panes with a shared folder tree, per-pane tabs, details / icon / thumbnail views, and a background copy/move queue. It is a native [Tauri](https://tauri.app/) 2 app: **filesystem work runs in Rust**, and the React 19 UI talks to it over typed IPC.

[feature-tour.webm](https://github.com/user-attachments/assets/8b8db964-8d03-44bc-b7f0-0beb31e6275b)


## Contents

- [Installation](#installation)
  - [System requirements](#system-requirements)
  - [Windows](#windows)
  - [macOS](#macos)
  - [Updating](#updating)
  - [Uninstalling](#uninstalling)
- [Features](#features)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Copy / move queue](#copy--move-queue)
- [Folder sizes](#folder-sizes)
- [Settings](#settings)
- [Development](#development)
- [Releasing](#releasing)
- [License](#license)

## Installation

Download the latest installer from **[GitHub Releases](https://github.com/omega123456/AxoPane/releases/latest)**.

### System requirements

| OS          | Supported                                      | Release asset                 |
| ----------- | ---------------------------------------------- | ----------------------------- |
| **Windows** | Windows 10 / 11, x64                           | `.exe` (NSIS setup) or `.msi` |
| **macOS**   | macOS 12 Monterey or later, Apple Silicon only | `.dmg`                        |

Intel Macs and Linux are not currently built.

### Windows

1. Download the `…-setup.exe` (recommended) or the `.msi` from the [latest release](https://github.com/omega123456/AxoPane/releases/latest).
2. Run the installer and follow the prompts. If SmartScreen warns about an unrecognised publisher, choose **More info → Run anyway**.
3. Launch **AxoPane** from the Start menu.

Notes:

- **WebView2** is required. It ships with Windows 10/11; the installer fetches it if it is missing.
- **Everything (optional)** — with [Everything](https://www.voidtools.com/) installed and running, AxoPane can size folders automatically while you browse. Without it, sizes are calculated on demand (see [Folder sizes](#folder-sizes)).
- **Windows 11** gets a native Mica window backdrop; Windows 10 uses a solid background.

### macOS

1. Download the `.dmg` from the [latest release](https://github.com/omega123456/AxoPane/releases/latest).
2. Open the disk image and drag **AxoPane** into **Applications**.
3. Clear the quarantine flag (the app is signed but **not Apple-notarized**, so Gatekeeper reports it as "damaged" or refuses to open it otherwise):
   ```bash
   xattr -dr com.apple.quarantine /Applications/AxoPane.app
   ```
4. Launch AxoPane from Applications or Spotlight.
5. **Recommended:** grant **Full Disk Access** in **System Settings → Privacy & Security → Full Disk Access** (add AxoPane with **+**, then restart it). Without it, macOS hides protected folders (Mail, Safari, other apps' containers, …) and background folder sizing skips them.

Release builds are signed with a stable self-signed identity (`AxoPane Local Code Signing`), so Full Disk Access and other privacy grants survive in-app updates. To confirm you have a properly signed build:

```bash
codesign -d -r- /Applications/AxoPane.app
```

The output should contain:

```text
identifier "com.axopane.app" and certificate leaf = H"a1006e17504884874fb91995cb27e336fc1bc348"
```

If it shows `cdhash` instead, the build was ad-hoc signed and privacy grants will not persist across updates.

### Updating

AxoPane checks GitHub Releases in the background (interval configurable in **Settings → Updates**, or **Off**). When an update is available, a banner appears; click **Download & install** and the app restarts into the new version. You can also check manually from **Settings → Updates**. Updates are signature-verified before install.

On macOS, updating in-app keeps the quarantine flag cleared — you only need step 3 above for the first install (or after manually downloading a new `.dmg`).

### Uninstalling

- **Windows** — **Settings → Apps → Installed apps → AxoPane → Uninstall**.
- **macOS** — quit AxoPane and move `/Applications/AxoPane.app` to the Trash. To remove settings and caches too, delete `~/Library/Application Support/com.axopane.app` and `~/Library/Caches/com.axopane.app`.

## Features

**Layout and navigation**

- **Dual panes** — two locations side by side with a resizable split, or single-pane mode
- **Shared folder tree** — lazy-loaded, follows the active pane; shows drives/volumes with capacity bars, **Favourites**, and the **Trash / Recycle Bin**
- **Per-pane tabs** — middle-click or **Ctrl+Enter** to open in a new tab; drag to reorder or move tabs between panes; lock tabs; close tabs to the left/right; add a tab's folder to Favourites
- **Views** — **Details**, **Icons**, and **Large thumbnails** (native OS thumbnails), chosen per tab
- **Details view** — Name, Size, Items, Type, Modified, Created; folders first, natural-sort names; show/hide and reorder columns
- **Breadcrumb path bar** — click segments, or edit the path directly; on Windows, environment variables such as `%USERPROFILE%` expand
- **History** — per-pane back/forward, **Backspace** to go up, open in the other pane / new tab
- **In-folder filtering** — start typing in a pane to filter; **Esc** clears
- **Details panel** — type, path, and dates for the current selection
- **Live updates** — open folders refresh through native filesystem watches, with no polling

**File operations**

- Copy, cut, paste, rename (filename selected without the extension), new folder / file, delete to Trash / Recycle Bin, delete permanently
- **OS clipboard integration** — copy in AxoPane and paste in Explorer / Finder, or the other way round
- **Drag and drop** — between panes, tabs, the tree, and Favourites; drag files out to other applications, with copy/move feedback
- **Copy / move queue** — background jobs with progress, conflict prompts, and folder merging on move (see [below](#copy--move-queue))
- **F5 / F6** copy or move the selection to the other pane
- **ZIP archives** — compress and extract from the context menu, preserving permissions and symlinks
- **Trash / Recycle Bin** — browse, restore items to their original location, empty
- **Properties** dialog; **Copy as path**; **Open with**, and on macOS change the default app for a file type
- **Native context menu items** — Windows shell extensions (classic and Windows 11) appear in AxoPane's context menu
- **Eject** removable volumes from the tree
- **Windows permissions** — read-only files are deleted cleanly, and access-denied errors offer to restart AxoPane as administrator

**Look and feel**

- Light, dark, or follow-system theme; adjustable zoom
- Configurable date format (absolute or relative, with optional time / seconds)
- Hidden files toggle
- Optional session restore (tabs, paths, sort, layout)
- Keyboard-first: every command has a remappable shortcut
- Built-in log viewer with adjustable log level

## Keyboard shortcuts

Shortcuts are editable in **Settings → Keybindings** (with conflict detection). Clipboard shortcuts (**Ctrl+C / X / V**) follow the OS and cannot be remapped. On macOS, **Ctrl** below means **⌘**.

| Action                | Windows              | macOS                            |
| --------------------- | -------------------- | -------------------------------- |
| Open                  | **Enter**            | **Enter**                        |
| Go up                 | **Backspace**        | **Backspace**                    |
| Refresh               | **Ctrl+R**           | **⌘R**                           |
| Rename                | **F2**               | **F2**                           |
| Delete (to Trash)     | **Delete**           | **F8** or **Delete**             |
| Delete permanently    | **Shift+Delete**     | **Shift+F8** or **Shift+Delete** |
| Copy to other pane    | **F5**               | **F5**                           |
| Move to other pane    | **F6**               | **F6**                           |
| New folder            | **Ctrl+Shift+N**     | **⌘⇧N**                          |
| Calculate folder size | **Space**            | **Space**                        |
| Open in new tab       | **Ctrl+Enter**       | **⌘Enter**                       |
| Open in other pane    | **Ctrl+Shift+Enter** | **⌘⇧Enter**                      |
| Select all            | **Ctrl+A**           | **⌘A**                           |
| Clear filter          | **Esc**              | **Esc**                          |
| Settings              | **Ctrl+,**           | **⌘,**                           |

**New file**, **Restore**, and **Empty Trash** have no default shortcut but can be bound.

Selection: **Ctrl+Click** toggles, **Shift+Click** selects a range.

## Copy / move queue

Copy, move, delete, and archive actions run as background jobs instead of blocking the UI.

- **One user action = one job.** Jobs on different volumes run in parallel; jobs sharing a volume are serialized.
- **Controls** — pause, resume, skip, cancel, and reorder pending jobs from the expanded queue panel.
- **Conflicts** — when a destination already exists, only that job pauses and asks **Replace**, **Skip**, **Rename**, or **Apply to all**. Moving a folder onto an existing one merges them.
- **Progress** — per-job bar, current file, throughput chart, and ETA.
- **Completion** — successful jobs auto-dismiss; failed jobs stay until cleared; cancel keeps files already copied.
- **App close** — AxoPane asks for confirmation if transfers are still running or pending.

## Folder sizes

| Platform                         | Behaviour                                                                                                                                        |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| **macOS**                        | A background folder-size index sizes folders as you browse; folders you open are sized right away. Grant Full Disk Access for complete coverage. |
| **Windows + Everything running** | Sizes populate progressively from the Everything index; size sorting works as values arrive.                                                     |
| **Windows without Everything**   | On demand: press **Space** on a folder, use **Calculate size** in the context menu, or **Calculate all folder sizes** in the toolbar.            |
| **Network locations**            | Always **N/A** — never calculated.                                                                                                               |

Automatic sizing can be turned off in **Settings → View and layout**; on-demand calculation always works.

## Settings

Open with **Ctrl+,** / **⌘,** or the gear icon.

- **View and layout** — hidden files, theme, zoom, default pane mode (dual / single), default view for new tabs, queue toast behaviour, session restore, automatic folder sizes
- **Columns** — show/hide and reorder detail columns
- **Dates** — date format, show time / seconds, relative dates
- **Keybindings** — search, inspect, and remap shortcuts
- **Updates** — current version, manual check, download & install, automatic check interval (**Off**, hourly, every 5 hours, daily, weekly)
- **Logs** — capture level and an in-app log viewer

## Development

### Requirements

- [Node.js](https://nodejs.org/) LTS and [pnpm](https://pnpm.io/) (`corepack enable`)
- [Rust](https://www.rust-lang.org/tools/install) stable
- Platform tooling from the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (WebView2 + MSVC build tools on Windows, Xcode Command Line Tools on macOS)
- For the full test suite: `cargo install cargo-nextest cargo-llvm-cov`, `rustup component add llvm-tools-preview`, and `pnpm exec playwright install chromium`

### Quick start

```bash
git clone https://github.com/omega123456/AxoPane.git
cd AxoPane
pnpm install
pnpm dev            # runs the app with the Vite dev server on port 1420
```

To build installable bundles for your own OS: `pnpm tauri build`.

### Scripts

| Command                       | Purpose                                                  |
| ----------------------------- | -------------------------------------------------------- |
| `pnpm dev`                    | Run the Tauri app in development                         |
| `pnpm build`                  | Build frontend + native app (no bundle)                  |
| `pnpm tauri build`            | Build installable bundles for your OS                    |
| `pnpm test` / `test:watch`    | Vitest                                                   |
| `pnpm test:coverage`          | Vitest with 90% coverage gates                           |
| `pnpm test:rust`              | Rust integration tests via cargo-nextest                 |
| `pnpm test:rust:coverage`     | Rust tests under cargo-llvm-cov with coverage gates      |
| `pnpm test:e2e`               | Playwright E2E + screenshot regression                   |
| `pnpm test:all`               | All of the above                                         |
| `pnpm lint` / `pnpm lint:fix` | ESLint                                                   |
| `pnpm format`                 | Prettier check                                           |
| `pnpm typecheck`              | TypeScript + `cargo check`                               |
| `pnpm clippy`                 | Rust clippy with warnings denied                         |
| `pnpm release:tauri-version`  | Interactive release helper (see [Releasing](#releasing)) |

### Stack

| Layer         | Technologies                                                                   |
| ------------- | ------------------------------------------------------------------------------ |
| Desktop shell | Tauri 2, Rust (filesystem, queue, watching, folder sizes, persistence)         |
| UI            | React 19, TypeScript, Vite, Zustand, TanStack Query / Virtual, Tailwind CSS v4 |
| Tests         | Vitest, Rust integration tests (nextest / llvm-cov), Playwright                |

### Project layout

```
AxoPane/
├── src/          # React app: components, lib (IPC wrappers), stores, types; tests in src/tests/
├── src-tauri/    # Rust backend, Tauri config, capabilities, icons; tests in src-tauri/tests/
├── e2e/          # Playwright specs and screenshot baselines
├── scripts/      # Build, coverage, and release helpers
└── CLAUDE.md     # Architecture, styling, and testing rules for contributors
```

### Contributing

Run `pnpm lint`, `pnpm typecheck`, and `pnpm test:all` before opening a PR. Every change must work on both Windows and macOS. See [CLAUDE.md](CLAUDE.md) for IPC conventions, styling constraints, and the screenshot baseline workflow.

## Releasing

[`.github/workflows/release.yml`](.github/workflows/release.yml) builds and publishes a GitHub Release (Windows x64 `.msi` / `.exe`, macOS Apple Silicon `.dmg`, plus updater bundles and `latest.json`) when a `v*` tag is pushed or the workflow is run manually.

1. Run **`pnpm release:tauri-version`**. It bumps the version, prompts for release notes (written to [`.github/tauri-release-body.md`](.github/tauri-release-body.md)), runs `pnpm build`, and only then commits, tags, and pushes.
2. Required repository secrets:
   - `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — updater signing.
   - `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `KEYCHAIN_PASSWORD` — the `AxoPane Local Code Signing` identity exported as `.p12` and base64-encoded (`openssl base64 -A -in AxoPaneLocalCodeSigning.p12 -out certificate-base64.txt`). CI verifies its SHA-1 is `A1006E17504884874FB91995CB27E336FC1BC348`.

## License

[MIT](LICENSE)
