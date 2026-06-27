# CLAUDE.md

Guidance for working in this repository. **Read this before implementing any phase.**

## Project

A **super-fast, lightweight, dual-pane cross-OS (Windows + macOS) file explorer** inspired by xplorer2.

- **Stack**: Tauri v2 · Rust · React 19 · TypeScript (strict) · Tailwind CSS v4 · pnpm.
- **Ownership split**: **Rust** owns filesystem/core logic, sorting, in-folder filtering, folder-size capability layer, copy/move queue, fs-watching, volume enumeration, persistence. **React** owns presentation + interaction state (Zustand + Tauri events + a thin typed `invoke()`; TanStack Query only for discrete request/response calls).

## Critical Rules for Agents

- **Tests live ONLY in dedicated roots.** Never bundle tests with production code: no `*.test.ts` / `*.spec.ts` next to sources under `src/`, no `describe`/`it` blocks inside application modules, and (Rust) no `#[cfg(test)]` or `#[test]` in `src-tauri/src/`. Use `src/tests/` (mirroring `src/`), `src-tauri/tests/`, and `e2e/` only.
- **Excluding functions or files from test coverage is strictly prohibited.** Maintain 90% lines/functions/statements for both TypeScript (Vitest v8) and Rust (`cargo-llvm-cov` `--fail-under-*`). Never lower thresholds or hide untested code — write tests instead. Forbidden: `istanbul`/`c8`/`v8` ignore comments, Vitest `coverage.exclude` / `coverage.include` tweaks that skip production code, Rust `#[cfg(coverage)]` / `#[cfg(not(coverage))]` beyond the existing Tauri entrypoint carve-out (`lib::run` / `main`), and any other exclude/ignore mechanism. No exceptions.
- **No fixed delays > 5 s** in any test. Use condition-based waiting (Playwright auto-wait, `waitFor`, `findBy*`, polling). Each individual test must complete in under 2 seconds.
- **Tests must never hit real machine-global APIs.** Rust tests run with `--features test-utils`; any code path that can touch vendor drivers, global OS settings, registry-backed settings, process-wide services, real GPUs, NVIDIA/NVAPI/DRS, or other machine-global state must compile to a fake/in-memory implementation or return `Unsupported` under `feature = "test-utils"` (and usually `coverage`). Never write tests that permit "real API success" as an acceptable branch for these integrations — assert the safe test fallback instead.
- **Package manager: `pnpm` only.**
- **Lint must be genuinely clean.** See the **Lint rule** above — zero warnings/errors repo-wide, no suppressions, including for pre-existing issues encountered during the session.
- **Vitest must run without React `act(...)` warnings.** Treat any `act(...)` stderr from Vitest as unfinished work — fix the test (e.g. `await userEvent`, `waitFor` / `findBy*`, wrap timer advances and IPC event emits in `act`, use the shared `ipc.emit` harness) rather than ignoring the warning. Pre-existing `act` warnings are in scope.
- **Vitest IPC mocking is mandatory and centralized.** All Vitest tests that touch Tauri IPC must use the shared harness in `src/tests/ipc-mock.ts` plus `ipc.override(...)` / `ipc.emit(...)`. Do **not** create ad hoc IPC mocks, per-test `mockIPC(...)` calls, direct `vi.mock()` stubs for `@tauri-apps/api/*` IPC modules, or direct mocks of `src/lib/*-commands.ts` command wrappers. If a command is missing from the default fixtures, add it to `src/tests/fixtures.ts` or override it in the test. **The intentional missing-mock failure (`[vitest] Unmocked Tauri IPC command: <cmd>`) is part of the contract and must not be bypassed.**
- **No raw `console` in frontend feature code.** Route all logging through `src/lib/app-log-commands.ts` (`logFrontend` and the toast helpers added later). `app-log-commands.ts` is the only module allowed to call `console` (as a last-resort sink). The `no-console` ESLint rule enforces this.
- **Critical instruction: any time you add or update a screenshot test or regenerate a screenshot baseline, you must inspect the resulting image and verify it matches expectations before you consider the change complete. Never accept an unreviewed screenshot baseline change.**
- **Screenshot tests cant take longer than 2 seconds to run each. Anything that takes longer is considered a failure.**
- **Rust and vitests cant take longer than 1 second per test suite. Anything that takes longer is considered a failure.**

**Permission debugging:** A runtime `forbidden`/permission error almost always means a missing or mistyped entry in `capabilities/default.json` or a mismatch between the TOML `commands.allow` name and the registered Rust name.

### Rust

- Never embed `#[cfg(test)]` or `#[test]` in `src-tauri/src/` — all tests go in `src-tauri/tests/` only.

## Styling constraint (NON-NEGOTIABLE)

- **Pure Tailwind utility classes only.**
- **No custom CSS rules. No `@apply`. No square-bracket arbitrary values** (e.g. `w-[237px]`, `text-[#abc]`).
- The Tailwind v4 **`@theme {}` block in `index.css` is the allowed design-system config** — every custom color/spacing/type-step/radius/shadow is a **named token** there, used as a normal utility (`bg-dark-window`, `w-tree`, `text-row`).
- Need a new value? Add a **named `@theme` token**, then use the generated utility. Never inline a bracket value.
- Add **`focus-visible` rings** to all interactive elements using the token palette (`focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border`, `ring-inset` on rows). The prototype lacks these — add them.
- **Light/dark theming = Tailwind class-based `dark:` variant** (not the prototype's JS `t`-helper). `index.css` contains only `@import "tailwindcss"`, `@custom-variant dark (&:where(.dark, .dark *));`, and `@theme { … }`. Components use **paired utilities** (`bg-light-window dark:bg-dark-window`, `text-light-text dark:text-dark-text`, …); the `theme` store toggles the `.dark` class on `<html>`. The `@custom-variant` directive is allowed config (like `@theme`), not a custom CSS rule.

## Testing Conventions

Keep every test in a dedicated file under the appropriate test root (`src/tests/`, `src-tauri/tests/`, `e2e/`). Production files must contain only shipping code.

### Vitest (TypeScript)

- Test files mirror source: `src/components/Foo.tsx` → `src/tests/components/Foo.test.tsx`.
- Setup file `src/tests/setup.ts` provides jsdom polyfills (`ResizeObserver`, `matchMedia`, `scrollIntoView`) and wires Vitest IPC through the shared `src/tests/ipc-mock.ts` harness. A missing IPC fixture throws `[vitest] Unmocked Tauri IPC command: <cmd>`.
- Always test real behavior through the public API with the shared harness. Use `ipc.override(...)` for per-test behavior and `ipc.emit(...)` for events. If a test needs a new IPC response, extend `src/tests/fixtures.ts` or override only that command in the test.
- After `render`, use `waitFor` / `findBy*` for async-mounted state. Use `const user = userEvent.setup()` and await interactions to avoid React `act(...)` warnings. **`pnpm test:coverage` must complete with zero `act(...)` warnings** — fix or wrap async updates (including `ipc.emit` and fake-timer advances) rather than leaving warnings in stderr.

### Rust

- Tests only in `src-tauri/tests/<area>_<focus>_integration.rs`. Name files after what they test, not meta-goals like `coverage_boost`.
- Use in-memory SQLite (`Connection::open_in_memory()`) — never mock the DB layer.
- Native integration boundaries that can mutate host/global state must be gated out of Rust tests with `feature = "test-utils"` and tested through fakes or explicit `Unsupported` assertions. This includes NVAPI/NVIDIA DRS preset writes, registry-backed driver settings, real vendor APIs, and similar machine-global APIs.
- After adding a new test file, register it in **both** aliases in the repo-root `.cargo/config.toml`: `gm-test-integration` and `gm-llvm-cov`.
- Run with `pnpm test:rust` for fast iteration; `pnpm test:rust:coverage` for the coverage gate (`cargo llvm-cov nextest`). `cargo-llvm-cov` sets `--cfg coverage` to exclude the Tauri runtime entrypoint (`lib::run` / `main`); do not add other code behind `cfg(coverage)` to dodge coverage.

### Playwright (E2E + Visual Regression)

- Specs live in `e2e/`. `pnpm test:e2e` (and therefore `pnpm test:all`) always includes `screenshots.spec.ts`.
- **Every new component / visible UI state needs screenshot coverage for both light and dark themes** in `e2e/screenshots.spec.ts`.
- **Do not increase Playwright screenshot pixel tolerance** (or any visual diff threshold in `playwright.config.mjs`) to make tests pass — fix the UI/regression or intentionally update baselines instead.
- Update the `VITE_PLAYWRIGHT` mock for any new IPC command called from the UI. **Never embed fixture data or domain logic inline in `playwright-ipc-mock.ts`** — all fixture data belongs in `src/tests/playwright-fixtures/` (one file per domain) and must be wired through the registry in `src/tests/playwright-fixtures/index.ts` so it can be looked up and overridden per-test without touching the mock router.
- After intentional visual changes, regenerate baselines: `pnpm test:e2e --update-snapshots` and commit the updated snapshot files.


## Commands

```
pnpm dev                # run the Tauri app in dev
pnpm build              # build frontend + app
pnpm test               # Vitest (run once)
pnpm test:coverage      # Vitest with v8 coverage (90% gate)
pnpm test:rust          # cargo-nextest integration tests
pnpm test:rust:coverage # cargo-llvm-cov (90/90/80 gates)
pnpm test:e2e           # Playwright visual-regression
pnpm lint / format / typecheck
```
