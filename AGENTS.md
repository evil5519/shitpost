# Repository Guidelines

## Project Overview

`shitpost` is a client-side personal portfolio built with eframe/egui 0.36. The same workspace builds a native desktop application and a WASM web application served by Trunk and deployed to GitHub Pages. The UI exposes About, Projects, Contact, Calculator, Text analyzer, and Color converter workspaces in a fixed six-item domain rail (n ≤ 6). On desktop the rail is a fixed left panel and each workspace opens as a movable egui window (or a non-overlapping pane in the explicit user-invoked tile mode); below 700 logical pixels the same workspaces render as one scrollable full-page view with a bottom thumb-zone navigation bar. A fixed identity anchor (brand → Home) and a command palette (Ctrl/Cmd+K, or the Search button) are the global anchors; the portfolio editor is an object-level action surfaced through each workspace's Action Hub, not a rail entry.

The app is local-only by design: it has no sync, notifications, permissions model, or cross-device conflict resolution. Those UMW-R surface contracts are explicitly unmet (single-writer local storage); see `docs/ux-compliance.md` for the full verdict and rationale.

`assets/sw.js` precaches `./shitpost.js` and `./shitpost_bg.wasm`; keep those names aligned with the crate name.

## Architecture & Data Flow

The workspace uses an enforced dependency direction:

```text
shitpost bootstrap -> ui -> core -> calculator-engine
```

`core` has no egui or eframe dependency. It owns domain state, deterministic business logic, validation, snapshots, migrations, and the single central `Command` enum. `core::CoreState` owns the active domain `View` and the four business slices:

- `core::portfolio`: portfolio/project state, URL/email validation, snapshots, and portfolio transitions.
- `core::calculator`: calculator input, history, persisted engine session, legacy-format migration, restore behavior, and calculator-engine integration. The live `CalculatorRuntime` is non-serializable and lives inside `core::CalculatorState`; `CalculatorState::from_snapshot` rebuilds it.
- `core::text_analyzer`: persisted text and deterministic character/word/line statistics.
- `core::color_converter`: persisted hex/RGB state, parsing, canonicalization, and structured `ColorError` values.

All feature events are direct variants of the one central `core::Command` enum. `CoreState::dispatch` applies commands and `CoreState::snapshot` returns framework-independent persisted data. `View` navigation is also dispatched through `Command::Navigate`.

`ui` depends on `core`, egui, and eframe. It contains rendering, responsive layout, egui focus/IDs/window geometry, transient workspace state, and translation from widget events into `core::Command`. UI does not mutate core domain fields directly. It adapts `eframe::Storage` to the core snapshot through `load_session`/`save_session`.

`ui` also owns the native/WASM bootstrap:

- `ui::run_native()` configures logging, native viewport (960×720, minimum 360×480), icon, and `eframe::run_native`.
- `ui::run_web()` configures `WebLogger`, locates `#the_canvas_id`, starts `WebRunner`, and removes or replaces `#loading_text`.
- `ui::PortfolioApp` is the eframe adapter and the only active renderer state holder.

`src/main.rs` is only a launcher and depends directly on `ui`:

```rust
#[cfg(not(target_arch = "wasm32"))]
fn main() -> ui::AppResult { ui::run_native() }

#[cfg(target_arch = "wasm32")]
fn main() { ui::run_web(); }
```

`src/lib.rs` is an empty package library target retained for Cargo compatibility; it contains no application module or public application API. The active library API is `crates/ui/src/lib.rs`.

Persistence is split physically: `core` defines serializable snapshots, versions, defaults, and migrations; `ui` performs the eframe storage adaptation. egui geometry, focus, window flags, and other visual workspace state are not persisted in core snapshots.

## Key Directories

| Path | Purpose |
|---|---|
| `crates/core/` | Framework-independent domain state, commands, validation, snapshots, migrations, and the four business slices. |
| `crates/ui/` | egui/eframe rendering, UI event translation, transient workspace state, storage adapter, and native/WASM launchers. |
| `crates/calculator-engine/` | Standalone scientific expression engine with no egui/eframe dependency. |
| `src/main.rs` | UI-only native/WASM launcher. |
| `src/lib.rs` | Empty root package library target; no application logic. |
| `assets/` | PWA service worker, manifests, and icons. |
| `.githooks/` | Versioned repository hooks. `.githooks/pre-commit` runs the full check gate. |
| `.github/workflows/` | CI, Pages deployment, and typo checking. |
| `docs/` | Architecture audit and related repository documentation. |
| `dist/` | Trunk output for deployment; generated and gitignored. |

## Development Commands

Native:

```sh
cargo run --release
```

Web development:

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
trunk serve
```

Open `http://127.0.0.1:8080/index.html#dev` during development so the service-worker registration is bypassed.

Release web build:

```sh
trunk build --release
```

Repository setup for the versioned hook, once per clone:

```sh
git config core.hooksPath .githooks
```

The hook is not activated automatically by clone; without this command Git will not run `.githooks/pre-commit`.

## Verification

Run the complete local gate with `NO_COLOR` unset:

```sh
env -u NO_COLOR ./check.sh
```

`check.sh` runs workspace checks, WASM compilation, rustfmt, Clippy with warnings denied, workspace tests, doctests, and `trunk build`. It is a single reproducible local gate, but it is **not an exact mirror of CI**: CI splits checks into separate jobs and uses different command arguments and target matrices. The `NO_COLOR` workaround is required because the checked Trunk version rejects a non-empty value.

Focused checks:

```sh
cargo test -p core
cargo test -p calculator-engine
cargo test -p ui
```

Core tests cover domain transitions, calculator persistence/legacy migration/runtime restoration, text statistics, and color parsing/canonicalization. UI tests cover the remaining UI storage/workspace adapter behavior. The gate proves compilation and automated tests, not native-window interaction or browser smoke behavior.

## Debugging the live UI

The optional `inspection` feature exposes the egui MCP inspection protocol for native development builds:

```sh
cargo build --features inspection
EGUI_INSPECTION=1 ./target/debug/shitpost
```

Attach to `127.0.0.1:5719` with egui MCP. Re-query the widget tree after layout changes because egui IDs can shift. Screenshots require a visible native window. Browser eframe output is a canvas, not semantic DOM controls; use screenshots and canvas interaction rather than DOM selectors.

## Code Conventions

- Keep domain behavior and validation in `core`; UI translates events and renders results.
- Keep `Command` as the single central command enum; add direct feature variants rather than nested per-slice command enums.
- `core` must not import egui or eframe.
- Keep visual workspace state in `ui`; `core` owns only the active domain `View` among navigation concerns.
- Use serde snapshots with defaults for persisted data and exclude runtimes/egui state.
- Use `cargo fmt`; CI treats Clippy warnings as errors.
- Avoid `unwrap` in production paths; tests use descriptive `expect`/`expect_err` messages.
- Keep public fallible APIs documented with `# Errors`.

## Runtime/Tooling Preferences

- Rust is pinned locally to `1.95.0` by `rust-toolchain`, including `rustfmt`, `clippy`, and the `wasm32-unknown-unknown` target.
- The Nix development shell is defined in `flake.nix`; it uses the stable Rust overlay with the WASM target, Trunk, GUI libraries, OpenSSL, and pkg-config.
- Cargo is the package manager; `Cargo.lock` is committed and workspace dependency resolution uses resolver 3.
- Trunk is not version-pinned in the repository; CI and local environments may resolve different Trunk releases.

## Current Risks

- `assets/sw.js` uses cache-first behavior with a fixed cache name and fixed WASM/JS filenames. Deployments can serve stale assets until the cache is invalidated.
- There is no automated browser smoke test; the full gate does not exercise native windows or browser canvas interaction.
- `src/lib.rs` can be removed: a temporary-copy experiment deleting it still passed `cargo check --workspace --all-targets` (exit code 0). Removing it from the real checkout remains explicit cleanup pending review.
- The versioned hook requires the one-time `git config core.hooksPath .githooks` setup step in every clone.

## Important Files

- `crates/core/src/lib.rs` — central commands, `CoreState`, dispatch, and composed persistence snapshots.
- `crates/core/src/session.rs` — snapshot schema, defaults, and migrations.
- `crates/core/src/portfolio.rs` — portfolio domain slice.
- `crates/core/src/calculator.rs` — calculator domain slice, runtime reconstruction, and legacy migration.
- `crates/core/src/text_analyzer.rs` — text analyzer state and deterministic statistics.
- `crates/core/src/color_converter.rs` — color state, parsing, canonicalization, and structured errors.
- `crates/ui/src/lib.rs` — storage adapter, `PortfolioApp` export, and native/WASM launchers.
- `crates/ui/src/app.rs` — egui shell: desktop rail, global anchors (brand, command palette), Action Hub sheets, tile mode, responsive mobile layout, event-to-command translation, and transient workspace state.
- `crates/calculator-engine/src/lib.rs` — standalone calculation engine and its tests.
- `src/main.rs` — two minimal UI-only `main` functions.
- `Cargo.toml` — workspace membership, root UI dependency, features, and workspace lints.
- `check.sh` — complete local verification gate.
- `.githooks/pre-commit` — versioned hook invoking `env -u NO_COLOR ./check.sh`.
- `docs/architecture-audit.md` — architecture audit and migration status.
- `assets/sw.js`, `assets/manifest.json` — PWA configuration.

## Deployment

Pushes to `main` trigger `.github/workflows/pages.yml`, which builds with Trunk and deploys `dist/` to GitHub Pages. Review the service-worker cache risk before deployments that change the generated WASM or JavaScript assets.