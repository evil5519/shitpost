# Repository Guidelines

## Project Overview

A client-side personal portfolio built with [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) / [egui](https://github.com/emilk/egui) 0.36. The same codebase runs as a native desktop app and as a WASM web app served via [Trunk](https://trunkrs.dev/) and deployed to GitHub Pages. Visitors get three portfolio views (About, Projects, Contact) and three interactive tools (Calculator, Text analyzer, Color converter). On desktop every destination opens as an independent movable/resizable in-app window; below 700 logical pixels the same destinations become one full-page scrollable view at a time.

> The repository directory is `shitpost` and the crate is named `shitpost` (template placeholder never filled). `assets/sw.js` precaches `./shitpost.js` and `./shitpost_bg.wasm` — keep these matching the crate name if it is ever renamed.

## Architecture & Data Flow

Immediate-mode egui: no component tree or state management library. eframe repaints each frame and calls `PortfolioApp::ui`, which mutates the app's fields directly through widget bindings and click handlers.

- **Entry** (`src/main.rs`): two `main` functions split by `#[cfg(target_arch = "wasm32")]`.
  - Native: `env_logger::init()`, `eframe::NativeOptions` (title `Portfolio`, 960×720, min 360×480, icon via `include_bytes!`), `eframe::run_native` with `shitpost::PortfolioApp::new`.
  - WASM: `eframe::WebLogger`, `wasm_bindgen_futures::spawn_local` grabs `#the_canvas_id` via `web_sys`, `eframe::WebRunner`, removes `#loading_text` (or crash message + `panic!`).
- **Crate root** (`src/lib.rs`): `#![warn(clippy::all, rust_2018_idioms)]`, private `mod app;`, `pub use app::PortfolioApp;` — the only public API.
- **App state** (`src/app.rs`): `PortfolioApp { portfolio: PortfolioContent, calculator: CalculatorState, text_analyzer: TextAnalyzerState, color_converter: ColorConverterState, #[serde(skip)] workspace: WorkspaceState }`.
  - `PortfolioContent { display_name, headline, about, projects: Vec<Project>, email, website, github }`, `Project { title, summary, url }` — all serde, `#[serde(default)]`, empty defaults. No seeded sample content: visitor views show neutral copy (`Use Edit portfolio to add an introduction.`, `No projects have been added yet.`, `No contact links have been added yet.`) until authored.
  - `WorkspaceState` holds `mobile_view: View`, one desktop open flag per non-Home `View`, and `Option<&'static str>` error slots for calculator/color validation. It is never serialized: every session starts Home with all windows closed and no errors. `eframe::App::persist_egui_memory` is overridden to `false`, so egui geometry/collapse memory is not restored either.
  - `new()` restores via `eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()`; `save()` writes back with `eframe::set_value`. Persisted: portfolio content + tool inputs/results (calculator operands/operation/result, analyzer text, color hex/RGB). Not persisted: window visibility, mobile route, validation errors, geometry.
- **Adaptive shell** (`PortfolioApp::ui` + `show_mobile`/`show_desktop`): `Self::is_mobile(ctx)` = `ctx.content_rect().width() < 700.0`. Desktop renders `Panel::top("top_panel")` (File→Quit native-only, Portfolio, Tools, theme buttons), a `CentralPanel` Home launcher, then one `egui::Window` per open flag via the `window(...)` helper (stable literal titles, `.open(&mut workspace.<flag>)`, staggered `default_pos`/`default_size`, `collapsible(false)`, `constrain(true)`). Mobile renders only the compact bar (Home when not on Home, Menu with the same 7 entries, theme buttons) plus the single active `View` in a vertical `ScrollArea`; no `egui::Window` is constructed.
  - `WorkspaceState::activate_view(view, mobile)` sets the desktop flag for every non-Home view, and the mobile route only in mobile mode. Desktop flags and the mobile route are independent: selecting on mobile remembers the desktop intent (`calculator_open` etc.), Home only clears the route.
- **Rendering**: one shared render function per destination, reused by both desktop windows and mobile pages — `show_home` (returns the clicked `View`), `show_about`/`show_projects`/`show_contact` (read-only), `show_portfolio_editor(&mut PortfolioContent)`, and per-tool renderers receiving their state plus the transient error slot.
- **Validation**: nonempty URLs render as `ui.hyperlink_to` only when they start `http://`/`https://`, otherwise plain text with editor hint `Links must start with http:// or https://.`; email renders `mailto:` when it contains `@`, else plain text with `Enter an email address containing @.`; inputs are never discarded. Calculator errors: `Enter valid numbers in both fields.`, `Cannot divide by zero.`, `Result is not finite.` Color: `Use a 6-digit hex color such as #336699.` hex input canonicalizes to uppercase `#RRGGBB`.

## Key Directories

|Path|Purpose|
|---|---|
|`src/`|All Rust code: `main.rs` (entry/glue), `lib.rs` (crate root), `app.rs` (state, pure tool logic, rendering, unit tests)|
|`assets/`|PWA/web assets: favicons, `manifest.json`, `site.webmanifest`, `sw.js` (service worker; copied into `dist/` by Trunk)|
|`.github/workflows/`|CI (`rust.yml`), Pages deploy (`pages.yml`), typo check (`typos.yml`)|
|`dist/`|Trunk build output for deployment (gitignored)|

## Development Commands

Native run:

```sh
cargo run --release
```

Web (dev server at `http://127.0.0.1:8080` — open `/index.html#dev`, the hash bypasses the service worker cache):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
trunk serve
```

Release web build → `dist/`:

```sh
trunk build --release
```

Full local verification gate (`check.sh`, `set -eux`):

```sh
env -u NO_COLOR ./check.sh
# cargo check --workspace --all-targets --all-features
# cargo check --workspace --all-features --lib --target wasm32-unknown-unknown
# cargo fmt --all -- --check
# cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::all
# cargo test --workspace --all-targets --all-features && cargo test --workspace --doc
# trunk build
```

> `check.sh` must be run with `NO_COLOR` unset: trunk 0.21.14 misparses a non-empty `NO_COLOR` value (`invalid value '1' for '--no-color'`, [trunk-rs/trunk#1065](https://github.com/trunk-rs/trunk/issues/1065)). CI workflows do not set `NO_COLOR`, so CI is unaffected.

### Debugging the live UI

The app can be driven live by the MCP server [`egui-mcp`](https://github.com/rerun-io/kittest_inspector/tree/main/crates/egui_mcp) (part of [rerun-io/kittest_inspector](https://github.com/rerun-io/kittest_inspector)): it speaks the `egui_inspection` protocol to a running app and exposes read-the-AccessKit-tree / click / type / scroll / drag / press-keys / screenshot / resize / wait_for tools. The app side is eframe's `inspection` feature, surfaced here as the `inspection` crate feature (declared in `Cargo.toml`, default off) so the capability exists only in development builds. The [egui_mcp README](https://github.com/rerun-io/kittest_inspector/blob/main/crates/egui_mcp/README.md) and the [`egui_inspection`](https://crates.io/crates/egui_inspection) crate are the authoritative docs.

```sh
cargo build --features inspection
EGUI_INSPECTION=1 ./target/debug/shitpost
# then: egui_attach { host: 127.0.0.1, port: 5719 } and drive via query_tree/click/type_text/screenshot
```

`EGUI_INSPECTION` semantics (verified against egui_inspection 0.36): unset or falsy (`0`/`false`) keeps inspection completely off; a truthy value (`1`/`true`) binds `127.0.0.1:5719`; any other value is taken as a `host:port` bind address (e.g. `0.0.0.0:5719` to expose across the network). Release builds omit the feature entirely unless `--features inspection` is passed explicitly; CI's `--all-features` checks cover it (`egui_inspection` compiles for wasm too).

Gotchas learned while driving the app through the MCP:

- **Screenshots need a visible window** (notably macOS): reading the tree and injecting input work while the app is backgrounded, but a fully occluded/minimized window renders no frame, so capture fails. Bring the window to the foreground first.
- egui auto-generated widget ids are counter-based: adding a conditional widget upstream (e.g. a validation error row appearing under a field) shifts every subsequent id on that pane. Re-query the tree before targeting actions after any layout-affecting change.
- The window title-bar close button mutates the flag passed to `.open(...)`. Windows must bind `&mut workspace.<flag>`, never a local — a local flag makes the close state evaporate next frame (window "reopens").
- After scrolling a pane, a locator click can resolve to a node that is no longer under the pointer (coordinates vs. scroll offset). Re-query, or focus via Tab from the previous field.
- `env_logger::init()` logs at info level only with `RUST_LOG=debug`; the "egui_inspection plugin attached" message is not visible by default. Check the port is listening (or just try `egui_attach`) instead of grepping logs.

## Code Conventions & Common Patterns

- **Naming**: standard Rust — `snake_case` functions/modules, `CamelCase` types. App code lives in `app.rs`; keep the module private and re-export the public type from `lib.rs`.
- **Formatting**: `rustfmt` via `cargo fmt` (checked in CI — must pass `--check`).
- **Lints**: `Cargo.toml` has an extensive `[workspace.lints]` table (`clippy::all` + dozens of pedantic lints as warn, `unsafe_code = "deny"`). `unwrap_used = "warn"` — tests must use `.expect(...)`/`.expect_err(...)` with messages; `map_err` closures must bind the error (`|_err|`) to satisfy `map_err_ignore`. CI elevates warnings to errors.
- **Persistence**: derive `Serialize`/`Deserialize` with `#[serde(default)]` on persisted structs; `#[serde(skip)]` on `WorkspaceState`; `#[serde(default)]` is struct-only (not valid on enums — `Operation` derives serde without it). Round-trip via `eframe::get_value`/`set_value` with `eframe::APP_KEY`; RON-backed. Override `persist_egui_memory() -> false` so windows don't resurrect across sessions.
- **UI**: menus inside `egui::MenuBar` inside `egui::Panel::top("top_panel")`; desktop subwindows via the private `window(&mut bool, title, pos, size, ctx, closure)` helper; `ui.close()` after a menu selection; `cfg!(target_arch = "wasm32")` for runtime branches (native-only File→Quit). No extra UI abstraction.
- **Styling**: `apply_style(ctx)` runs at the top of `ui()` every frame and uses `ctx.all_styles_mut` to keep both dark and light themes in sync: teal accent (`ACCENT_DARK #2DD4BF` / `ACCENT_LIGHT #0D9488`) for hyperlinks/selection/section labels, 6–10 px corner radii, and a spacing scale (`item_spacing` 8, `button_padding` 10×6). Home renders a hero (name 32 strong + muted headline) followed by two card grids — 3 columns on desktop, 2 on mobile — built from `Destination { view, title, description }` and `destination_card` (a `egui::Button` over a `LayoutJob`, `min_size`, `corner_radius(8)`, `wrap_mode(Wrap)`). Panels get padding via `Frame::new().fill(ui.visuals().panel_fill).inner_margin(Margin::same(28|16))`.
- **egui 0.36 API traps** (all hit while building the UI): theme is a dual-style system — use `ctx.all_styles_mut`/`set_style_of(Theme)` (there is no `ctx.set_style`); `Visuals::panel_fill` is a **field** while `visuals.window_fill()` is a **method**; `Frame` has no `.sense()` (use a `Button` for clickable cards); `CentralPanel` padding is set via `.frame(Frame)`, not `.inner_margin`; `#[serde(default)]` is struct-only; Grid ids must be unique per grid instance. Cards must use a fill distinct from the panel (`window_fill` ≈ `panel_fill` in light theme and disappears) — egui's default button fill works in both themes.
- **PWA**: `assets/sw.js` precaches `./`, `index.html`, `./shitpost.js`, `./shitpost_bg.wasm` (cache-first). `Trunk.toml` sets `filehash = false`. `site.webmanifest` is currently not linked by `index.html`; `manifest.json` is (name `Personal Portfolio`).

## Important Files

- `src/main.rs` — native + wasm entry points, platform glue; native title/size and both `shitpost::PortfolioApp::new` callsites live here.
- `src/lib.rs` — crate root, public API surface (`pub use app::PortfolioApp`).
- `src/app.rs` — all state, persistence, pure tool logic, adaptive shell, rendering, and the inline unit tests.
- `Cargo.toml` — eframe/egui 0.36, serde (derive), `log`; `inspection` feature for dev; edition 2024, `rust-version = "1.95"`; lints table.
- `rust-toolchain` — pinned `1.95.0` with `rustfmt`, `clippy`, and `wasm32-unknown-unknown` target.
- `Trunk.toml` — `[build] filehash = false`
- `index.html` — Trunk entry: `<title>Portfolio</title>`, description meta, canvas `#the_canvas_id`, `#loading_text`, service worker registration (skipped with `#dev` hash).
- `check.sh` — local verification gate mirroring CI (see NO_COLOR caveat above).
- `.github/workflows/rust.yml` / `pages.yml` / `typos.yml` — CI (`RUSTFLAGS=-D warnings`), Pages deploy, PR typo check.
- `flake.nix` — Nix dev shell (rust-overlay stable, Trunk, GUI/OpenSSL libs).
- `assets/sw.js`, `assets/manifest.json` — PWA configuration (crate-derived JS/WASM filenames).

## Runtime/Tooling Preferences

- **Rust**: pinned `1.95.0` via `rust-toolchain`; edition 2024; `wasm32-unknown-unknown` target required for web builds.
- **Package manager**: Cargo with `Cargo.lock` committed. The lockfile keeps `egui_inspection` and its deps for the dev feature; release resolution excludes them.
- **Web toolchain**: Trunk (not version-pinned in-repo; CI downloads "latest"). No Node/JS toolchain required.
- **Nix** (`flake.nix`, `nixos-unstable` + `rust-overlay`) is the preferred way to get a working shell; otherwise install system GUI deps listed above for native builds on Linux.

## Testing & QA

- **Unit tests** live inline in `src/app.rs` (`#[cfg(test)] mod tests`): pure contract tests for `calculate` (add, invalid operands, ±0 division, overflow), `analyze_text` (empty, Unicode, repeated whitespace, trailing newline), `parse_hex_color`/`format_hex` (prefixed/lowercase/valid lengths/non-hex), and a persistence round-trip through a test-only in-memory `eframe::Storage` (`HashMap<String, String>` + `get_string`/`set_string`/`remove_string`/`flush`) proving content/tool data survive while `WorkspaceState` resets. Currently 14 tests.
- **CI gates** (`.github/workflows/rust.yml`): formatting, clippy `-D warnings`, warnings-as-errors via `RUSTFLAGS`/`RUSTDOCFLAGS`, wasm compilation check, native tests, and a cross-compile release matrix. `--all-features` runs cover the `inspection` feature in checks.
- **Typo check**: `crate-ci/typos` action on every PR; `.typos.toml` extends the dictionary with `egui`.
- **Coverage**: not configured.
- **Deployment**: push to `main` triggers `pages.yml` → `trunk build --release --public-url https://<owner>.github.io/<repo>` → `dist/` deployed to `gh-pages`.