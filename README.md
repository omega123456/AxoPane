# AxoPane

A cross-platform **dual-pane desktop file explorer** for **Windows and macOS**, inspired by xplorer2. AxoPane pairs two independent file panes with a shared folder tree, per-pane tabs, a classic details view, breadcrumb navigation, and a background copy/move queue. The UI is a native [Tauri](https://tauri.app/) 2 shell with [React](https://react.dev/) 19 (TypeScript). **Filesystem work runs in Rust**; the frontend handles presentation and interaction state through typed Tauri IPC (`invoke`) and events.

## Contents

- **Install & downloads**
  - [GitHub releases](#github-releases-ci)
  - [Install by OS](#install-by-operating-system)
  - [macOS Gatekeeper / quarantine](#macos-quarantine-exclusion-step-by-step)
- **Using the app**
  - [How it works](#how-it-works)
  - [Features](#features)
  - [Keyboard shortcuts](#keyboard-shortcuts)
  - [Copy / move queue](#copy--move-queue)
  - [Folder sizes](#folder-sizes)
  - [Settings](#settings)
- **Develop locally**
  - [Requirements](#requirements)
  - [Setup](#setup)
  - [Quick start](#quick-start)
  - [Scripts](#scripts)
- **Reference**
  - [Stack](#stack)
  - [Project layout](#project-layout)
  - [Contributing](#contributing)
  - [License](LICENSE)
  - [CLAUDE.md](CLAUDE.md)

## How it works

AxoPane splits responsibilities between a Rust backend and a React frontend:

| Layer | Responsibility |
| ----- | -------------- |
| **Rust (Tauri)** | Directory listing, sorting, in-folder filtering, folder-size capability, copy/move queue, filesystem watching, volume enumeration, persistence |
| **React** | Dual-pane shell, folder tree, tabs, details grid, breadcrumbs, transfer queue UI, settings modal, keyboard handling |

When you open a folder, the active pane asks Rust for entries. Sorting and filtering happen on the backend so large directories stay responsive; the details list is virtualized in the UI. Filesystem changes on watched paths arrive as incremental patches instead of full reloads.

Each pane maintains its own tabs, current path, sort order, and filter. A shared folder tree on the left follows whichever pane is active. Copy and move operations are queued jobs with per-volume concurrency, conflict prompts, and a bottom-right progress panel.

On **Windows**, folder sizes can be filled eagerly from the [Everything](https://www.voidtools.com/) index when the Everything service is running. On **macOS** (and on Windows without Everything), sizes are computed on demand when you press **Space** on a selected folder.

## Features

- **Dual panes** — browse two locations side by side; resize the split; switch to single-pane mode in Settings
- **Shared folder tree** — lazy-loaded tree that tracks the active pane; expand/collapse nodes; open paths from the tree
- **Per-pane tabs** — independent tab sets per pane; middle-click a folder to open it in a new tab; close tabs from the tab bar or context menu
- **Classic details view** — columns for Name, Size, Items, Type, Modified, and Created; per-pane sorting with folders always first and natural-sort names; show/hide and reorder columns in Settings
- **In-folder filtering** — type to filter the current directory, or use the filter bar; **Esc** clears the filter
- **Folder sizes** — eager progressive sizes on Windows with Everything; manual **Space** calculation elsewhere; network paths always show **N/A**
- **Copy / move queue** — background transfers with progress, speed, ETA, pause/resume/cancel, job reordering, and per-conflict Replace / Skip / Rename / Apply-to-all prompts
- **Cross-pane transfers** — **F5** copies and **F6** moves from the active pane to the other pane (with confirmation)
- **Filesystem watching** — active tabs update incrementally when files change; background tabs recheck when activated
- **File operations** — copy, cut, paste, rename, delete, new folder, new file via keyboard, toolbar, or context menu
- **Selection** — **Ctrl+Click** toggles selection; **Shift+Click** range-selects; **Ctrl+A** selects all
- **Navigation** — breadcrumb path bar, **Backspace** to go up, per-pane back/forward history, open in other pane / new tab
- **Hidden files** — global toggle in the command bar
- **Session restore** — optional restore of open tabs, paths, sort, and layout on startup
- **Light / dark theme** — toggle from the command bar; preference persisted locally
- **Keyboard-first UX** — shared default shortcuts across OSes, fully customizable in Settings (clipboard shortcuts stay on OS defaults)
- **Software updates** — background update checks, in-app download/install on Windows, update banner and Settings panel
- **Native desktop app** — Tauri shell with native window decorations; smaller footprint than typical Electron stacks

### Out of scope (v1)

File previews/thumbnails, global search, drag-and-drop between panes, batch rename tools, plugins, and Linux builds.

## Keyboard shortcuts

Default shortcuts are editable in **Settings → Keybindings**. Clipboard commands (**Ctrl+C / X / V**) follow the platform defaults and cannot be remapped.

| Action | Default |
| ------ | ------- |
| Open | **Enter** |
| Go up | **Backspace** |
| Refresh | **Ctrl+R** |
| Rename | **F2** |
| Delete | **Delete** |
| Copy to other pane | **F5** |
| Move to other pane | **F6** |
| New folder | **Ctrl+Shift+N** |
| Calculate folder size | **Space** |
| Open in new tab | **Ctrl+Enter** |
| Open in other pane | **Ctrl+Shift+Enter** |
| Select all | **Ctrl+A** |
| Clear filter | **Esc** |
| Settings | **Ctrl+,** |

On macOS, **Ctrl** in the table above is shown as **⌘** in the UI.

Start typing in a pane (when not focused in an input) to jump to the filter field.

## Copy / move queue

Copy and move actions enqueue background jobs instead of blocking the UI.

- **One user action = one job.** Jobs that touch different volumes can run in parallel; jobs sharing a volume are serialized.
- **Controls** — pause, resume, cancel, reorder pending jobs from the expanded queue panel.
- **Conflicts** — when a destination file already exists, the affected job pauses and prompts for **Replace**, **Skip**, **Rename**, or **Apply to all**; other jobs keep running.
- **Progress** — per-job bar, current file name, throughput, and ETA once it stabilizes.
- **Completion** — successful jobs auto-dismiss after a delay; failed jobs stay until cleared; cancel stops as soon as possible and keeps files already copied.
- **App close** — AxoPane warns and asks for confirmation if transfers are still active or pending.

Open the queue from the bottom-right toast or expand it to manage all jobs.

## Folder sizes

| Platform | Behavior |
| -------- | -------- |
| **Windows + Everything running** | Sizes populate progressively for the full directory; size sorting works as values arrive |
| **Windows without Everything** | Press **Space** on a folder (or use **Calculate size** in the context menu) for an on-demand recursive scan |
| **macOS** | Same manual on-demand behavior as Windows without Everything |
| **Network drives / folders** | Always **N/A** — never calculated |

If Everything is not available on Windows, a banner offers a download link and can be dismissed.

## Settings

Settings opens as an in-app modal with sidebar navigation:

- **View & Layout** — single vs dual pane, tree width, pane split, details panel visibility, zoom, session restore, theme, hidden files, update check frequency
- **Columns** — show/hide and reorder detail columns
- **Keybindings** — inspect and remap shortcuts; conflict detection for duplicate bindings
- **Updates** — current version, manual check, install downloaded updates, automatic check interval (**Off**, every hour, 5 hours, day, or 7 days)

## Stack

| Layer | Technologies |
| ----- | ------------ |
| Desktop shell | Tauri 2, Rust (filesystem, queue, watching, persistence) |
| UI | React 19, TypeScript, Vite 8, Zustand, Tailwind CSS v4 |
| Virtualization | `@tanstack/react-virtual` |
| Updates | `@tauri-apps/plugin-updater` |
| Tests | Vitest (coverage gates), Rust integration tests (nextest / llvm-cov), Playwright E2E + screenshot baselines |

## Requirements

| Tool | Notes |
| ---- | ----- |
| [Node.js](https://nodejs.org/) | LTS recommended |
| [pnpm](https://pnpm.io/) | Package manager (`corepack enable` or install globally) |
| [Rust](https://www.rust-lang.org/tools/install) | Required to build the Tauri backend |
| [cargo-nextest](https://nexte.st/book/installing.html) | For `pnpm test:rust` and `pnpm test:all` |
| Rust coverage (optional) | For `pnpm test:rust:coverage` / `pnpm test:all`: `cargo install cargo-llvm-cov` and `rustup component add llvm-tools-preview` |
| OS deps | See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform |

**Windows only (optional, for eager folder sizes):** [Everything](https://www.voidtools.com/) installed and running.

## Setup

Follow these steps on a new machine before **Quick start** or **Contributing**.

1. **Node.js** — Install [Node.js](https://nodejs.org/) (LTS). Verify with `node -v`.
2. **pnpm** — Enable via Corepack (`corepack enable` then `corepack prepare pnpm@latest --activate`) or [install pnpm](https://pnpm.io/installation) globally. Verify with `pnpm -v`.
3. **Rust** — Install [rustup](https://www.rust-lang.org/tools/install) and the stable toolchain. Verify with `cargo -v` and `rustc -V`.
4. **Tauri OS dependencies** — Install platform tools from [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (WebView2 on Windows, Xcode CLT on macOS, etc.).
5. **Clone and install JS deps** — From the repo root:
   ```bash
   git clone <repository-url>
   cd file-explorer
   pnpm install
   ```
6. **Playwright (for E2E / `pnpm test:all`)** — Install Chromium once:
   ```bash
   pnpm exec playwright install chromium
   ```
7. **cargo-nextest** — Required for Rust integration tests:
   ```bash
   cargo install cargo-nextest
   ```
8. **Rust coverage tools (optional)** — For the full CI-equivalent test run:
   ```bash
   rustup component add llvm-tools-preview
   cargo install cargo-llvm-cov
   ```

For day-to-day development, steps 1–5 and **Quick start** are enough.

## Quick start

From the repository root (after **[Setup](#setup)** if this is a fresh clone):

```bash
pnpm install
pnpm dev
```

The dev server uses port **1420** (`http://127.0.0.1:1420`).

## Scripts

| Command | Purpose |
| ------- | ------- |
| `pnpm dev` | Run the full Tauri app in development |
| `pnpm build` | Build frontend + native app (no bundle) |
| `pnpm preview` | Preview the built frontend |
| `pnpm tauri build` | Build installable bundles for your OS |
| `pnpm release:tauri-version` | Interactive release helper: bumps version, prompts for release notes, runs `pnpm build`, then commits, tags `v*`, and pushes |
| `pnpm test` | Run Vitest once |
| `pnpm test:watch` | Vitest in watch mode |
| `pnpm test:coverage` | Vitest with coverage thresholds |
| `pnpm test:rust` | Rust integration tests via cargo-nextest |
| `pnpm test:rust:coverage` | Same tests under cargo-llvm-cov |
| `pnpm test:all` | Vitest coverage + Rust llvm-cov + Playwright E2E |
| `pnpm test:e2e` | Playwright E2E tests (including visual regression) |
| `pnpm lint` / `pnpm lint:fix` | ESLint |
| `pnpm format` | Prettier check |
| `pnpm typecheck` | TypeScript + `cargo check` |
| `pnpm clippy` | Rust clippy with warnings denied |

## GitHub releases (CI)

The workflow [`.github/workflows/release.yml`](.github/workflows/release.yml) publishes GitHub Release assets for these platforms:

| Platform | Artifacts | Notes |
| -------- | --------- | ----- |
| **Windows (x64)** | Updater bundle + `.msi` / `.exe` (NSIS) installers | Signed when `TAURI_SIGNING_PRIVATE_KEY` secrets are configured; in-app updates download first, then restart to finish |
| **macOS (Apple Silicon)** | `.dmg` | **Unsigned in v1** — no Apple code signing or notarization yet; users may see Gatekeeper warnings |

The release workflow runs on **`workflow_dispatch`** (Actions → Release → Run workflow) or when you push a version tag matching `v*` (for example `v0.1.0`).

1. From the repo root, run **`pnpm release:tauri-version`**. It bumps **`version`** in [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json), prompts for release notes, writes [`.github/tauri-release-body.md`](.github/tauri-release-body.md), runs **`pnpm build`** first, and only commits/tags/pushes on success.
2. Or bump `tauri.conf.json` yourself, commit, create and push the tag (`git tag v0.1.0 && git push origin v0.1.0`), or run the workflow manually after tagging.
3. Before the **first signed Windows release**, replace placeholder updater values in `tauri.conf.json` — see [`.github/RELEASE_CHECKLIST.md`](.github/RELEASE_CHECKLIST.md).

Releases are published directly (non-draft) by default.

> See the release assets to download installers for Windows and the unsigned macOS disk image.

## Install by operating system

Download installers from **GitHub Releases** for your repository. Use the table below, then see the linked sections for platform-specific notes.

| OS | Artifacts (CI) | Install | In-app updates | More detail |
| -- | -------------- | ------- | -------------- | ----------- |
| **Windows (x64)** | `.msi`, `.exe`, updater bundle | Run the installer from the release asset | Yes — download then restart to finish | [GitHub releases](#github-releases-ci) |
| **macOS (Apple Silicon)** | `.dmg` | Open the `.dmg` and drag **AxoPane** to **Applications** | Limited on unsigned builds | [macOS quarantine](#macos-quarantine-exclusion-step-by-step) · [Tauri macOS signing](https://v2.tauri.app/distribute/sign-macos/) |

For **building from source** on any supported OS, use [Requirements](#requirements), [Setup](#setup), and [Quick start](#quick-start) instead of prebuilt installers.

### Windows prerequisites

- **WebView2** — usually already installed on Windows 10/11; Tauri bundles it when needed for older systems.
- **Everything (optional)** — install [Everything](https://www.voidtools.com/) and keep it running for instant folder sizes and size sorting in large directories.

### Windows installation

1. Download the `.msi` or `.exe` installer from GitHub Releases.
2. Run the installer and follow the prompts.
3. Launch **AxoPane** from the Start menu or desktop shortcut.

### macOS installation

1. Download the `.dmg` from GitHub Releases.
2. Open the disk image and drag **AxoPane** into **Applications**.
3. If macOS blocks the app because it is unsigned, follow [macOS quarantine exclusion](#macos-quarantine-exclusion-step-by-step) below.

macOS builds from CI are **unsigned** in v1 (see [`.github/RELEASE_CHECKLIST.md`](.github/RELEASE_CHECKLIST.md)). Gatekeeper may report the app as damaged or refuse to open it until quarantine is cleared or you use **Open** from the context menu.

## macOS quarantine exclusion (step by step)

If macOS blocks AxoPane because it is unsigned (for example, "app is damaged" or "cannot be opened"), remove quarantine attributes from the app bundle.

1. Move the app to a stable location, such as `/Applications/AxoPane.app`.
2. Open Terminal.
3. Verify the quarantine flag is present:
   ```bash
   xattr -l "/Applications/AxoPane.app"
   ```
4. Remove the quarantine attribute recursively:
   ```bash
   xattr -dr com.apple.quarantine "/Applications/AxoPane.app"
   ```
5. Confirm the attribute is gone:
   ```bash
   xattr -l "/Applications/AxoPane.app"
   ```
   If nothing prints for `com.apple.quarantine`, quarantine is removed.
6. Start the app from Finder. If Gatekeeper still prompts, right-click the app, choose **Open**, then confirm **Open**.

Use this only for binaries you trust.

## Project layout

```
file-explorer/
├── src/                 # React app: components, lib (IPC wrappers), stores, styles, types
├── src-tauri/           # Rust backend, Tauri config, permissions, persistence, icons
├── e2e/                 # Playwright specs (including visual regression)
├── package.json         # Frontend scripts and dependencies
└── CLAUDE.md            # Maintainer/agent notes: architecture, commands, testing gates
```

## Contributing

1. Complete **[Setup](#setup)** (including Playwright, cargo-nextest, and Rust coverage tools if you run the full suite), then stay on the latest dependencies with `pnpm install` as needed.
2. Run `pnpm lint`, `pnpm typecheck`, and `pnpm test:all` (Vitest coverage, Rust with llvm-cov, Playwright) before opening a PR.
3. For behavior that depends on the native shell, verify with `pnpm dev` when possible. See **[CLAUDE.md](CLAUDE.md)** for IPC conventions, styling constraints, and screenshot baseline workflow.

## License

This project is licensed under the [MIT License](LICENSE).
