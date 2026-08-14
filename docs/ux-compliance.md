# UMW-R Compliance Audit — shitpost

**Scope:** validation of the shitpost portfolio (eframe/egui 0.36, native + WASM) against the
Unified Matrix Workspace, Revised (UMW-R) architectural standard.

**Date:** 2026-08-13. **Method:** source inspection, unit tests (`cargo test`), interactive smoke
tests against the live native app via the egui MCP inspection protocol, and the repository gate
(`env -u NO_COLOR ./check.sh`).

## Verdict summary

| Invariant | Verdict | Evidence |
|---|---|---|
| 1. Taxonomies identical across surfaces | **Compliant** | The same `PORTFOLIO_MENU_ITEMS`/`TOOL_MENU_ITEMS` arrays drive the desktop rail, mobile bottom sheets, Home cards, and the command palette. "Edit portfolio" is an action, not a rail entry, on every surface. |
| 2. State/permissions/data integrity identical everywhere | **Compliant (single-writer)** | One `core::CoreState` + one snapshot codec (`crates/core/src/session.rs`) feeds both native and WASM. No permissions exist — every operation is owned by the single local user. |
| 3. Focused workspaces on shared data objects | **Compliant** | Six workspaces operate on shared persisted objects (portfolio, calculator session, text, color) through the single `core::Command` stream. |
| 4. Global anchors fixed position/function | **Partially met** | Identity anchor (brand → Home) and command/search palette (Ctrl/Cmd+K) are fixed on every surface. Sync and notifications do not exist (local-only; see Unmet Contracts). |
| 5. Domain Rail n ≤ 6 visible | **Compliant** | Exactly 6 rail entries, grouped Portfolio/Tools (3 + 3). Overflow/role-filtering N/A at 6 items. |
| 6. Deterministic adaptation | **Compliant** | The only mode switch is `content width < 700` (screen geometry). No inference-based adaptation. |
| 7. Desktop split/multi-pane | **Compliant** | Explicit user-invoked tile mode (top-bar "Tile"/"Untile", palette entry): open workspaces become fixed non-overlapping panes; default is free-floating windows. |
| 8. Keyboard accessibility (desktop/web) | **Partially met** | Ctrl/Cmd+K palette reaches every destination and action; menus and text fields are keyboard-reachable; egui TextEdit provides in-widget Ctrl+Z/Y. Window close buttons, tile toggle, and theme buttons have no keyboard shortcut (pointer-only by egui design). |
| 9. Evidence-based superiority claims | **Compliant** | This report cites only named HCI principles (Hick-Hyman, Fitts, Cognitive Load Theory, Information Foraging) applied to concrete controls; where no usability data exists it is said so explicitly. |

## Unmet contracts (explicit, not approximated)

- **Sync / cross-device conflict resolution (§4).** The app is client-side only by design; there is
  no backend, no network, and no shared object can be edited from two devices. A conflict-
  resolution strategy is therefore N/A, not implemented. Shipping a simulated "sync" anchor would
  be decorative UI and is refused under the standard's own standing behavior.
- **Notifications anchor (Invariant 4).** No notification system exists; a fixed anchor for a
  nonexistent function would be decorative. The function does not exist on any surface.
- **Permissions (Invariant 2).** Single local user; no roles/tenants. Role-filtered rail
  overflow is N/A.
- **Action Hub for pure render workspaces.** Text analyzer and Color converter have no
  object-level actions (their object is edited in place); they intentionally expose no Action Hub
  button. Calculator's hub was added via the new `Command::CalculatorClearHistory`.

## Workflows audited

The six destinations map to these workflows: browse identity (About), browse projects (Projects),
browse contact (Contact), calculate (Calculator), analyze text (Text analyzer), convert color
(Color converter) — plus the cross-cutting edit-portfolio action.

---

### 1. Information Architecture Mapping

| Workflow | Domain Rail entry | Canvas engine | Rationale |
|---|---|---|---|
| About | About (Portfolio group) | Document | Free-text identity content; read + edit action. |
| Projects | Projects (Portfolio group) | Document/list | Ordered project objects with title/summary/URL. |
| Contact | Contact (Portfolio group) | Document | Link collection; validation errors inline. |
| Calculator | Calculator (Tools group) | Form + REPL | Text input, history transcript, completion chips. |
| Text analyzer | Text analyzer (Tools group) | Form | Single live-statistics text object. |
| Color converter | Color converter (Tools group) | Form | Hex/RGB inputs + live preview swatch. |

New-domain justification: none needed — every workflow nests under an existing rail entry.

**Rail cap / hierarchy check:** exactly 6 visible entries (n = 6 ≤ 6) — no overflow, no pinning
needed. In-canvas hierarchy stays ≤ 2 levels (workspace → content). **Edit portfolio** was removed
from the rail (was entry 7) and is now an **Action Hub sheet** action on the About, Projects, and
Contact workspaces; it is also reachable from the command palette.

**Action Hub sheet contents:**

| Workspace | Actions | Confirmation / staging |
|---|---|---|
| About | Edit portfolio | None — opens the editor; edits are per-keystroke persisted. |
| Projects | Add project, Edit portfolio | None — Add project creates an empty, editable object (non-destructive). |
| Contact | Edit portfolio | None. |
| Calculator | Clear history | None — clears the ≤ 100-entry transcript only; variables/definitions survive. |
| Text analyzer / Color converter | (none — no object-level actions exist) | — |

Action Hub entries are transient sheets anchored to the workspace header, never blocking modals.

---

### 2. Cross-Device Layout Matrix

| Control | Mobile (bottom thumb-zone, transient sheets) | Desktop (edge-anchored, precision) |
|---|---|---|
| Identity anchor (brand) | Fixed top-left of top bar; tap → Home | Fixed top-left, next to File; click → Home |
| Navigation (rail) | Bottom bar: "Portfolio" / "Tools" group buttons → transient sheets above the bar (≤ 3 choices each) | Fixed left-edge rail panel, 6 entries grouped Portfolio/Tools; click opens the workspace window |
| Command palette (search/command anchor) | "Search" button in bottom bar; palette modal centered | Ctrl/Cmd+K or "Search" button (top-right cluster); palette modal centered |
| Theme control | Top-right of top bar | Top-right cluster (fixed position, shared function) |
| Tile/split mode | N/A (single full-page workspace — no concurrent panes) | Explicit "Tile"/"Untile" toggle in top-right cluster + palette entry |
| Workspace content | One full-page scrollable view | Window or tiled pane, vscroll-enabled |
| Home launcher | Central panel cards (2-col grid) | Central panel cards (3-col grid), behind/alongside windows |

Platform-exclusivity notes: tile mode is desktop-only because mobile is single-task by definition
(no concurrent panes); theme is exposed on both surfaces at fixed positions. No control exists on
only one platform without a stated counterpart or justification.

---

### 3. Interaction Cost Breakdown

Primary task: open a workspace and use it.

| Platform | Steps to open + use |
|---|---|
| Desktop, mouse | 1. Click rail entry → window opens (or 1. Click Home card). 0 extra steps to use content. |
| Desktop, keyboard | 1. Ctrl/Cmd+K → type query → Enter. |
| Mobile | 1. Tap group button → 2. tap workspace in sheet (2 steps; sheets are transient). 0 extra steps to use content. |

**Modality transitions:** none required — every path stays in one modality (pointer or keyboard or
touch). Tap → keyboard is available but not forced (palette is optional).

**Hick-Hyman:** the mobile navigation sheet presents ≤ 3 co-equal choices (each group); the desktop
rail groups 6 items into two labeled sections of 3, and the palette filters by query. No decision
point exceeds 5 co-equal choices.

**Fitts:** desktop rail sits at the left screen edge and the top-bar cluster at the top edge (both
within 1-2 px of screen/perimeter targets); palette and windows center on the current visual focus.
Mobile group buttons sit in the bottom thumb arc (below half the viewport height).

**Expert path:** the command palette is the expert path and exists (Home, all 6 workspaces, Edit
portfolio, Tile toggle, Quit — native only). This is a genuine improvement over the previous
menu-only navigation, which had no keyboard path. Established by design review, not by usability
data — no usability test was run.

**User type:** intermediate default path (visible rail/sheets); expert path (palette). Novice
single-path flow exists via Home cards.

---

### 4. State Machine & Error Recovery Rules

- **Local vs. synced:** all state is local (`eframe::Storage` → browser localStorage on WASM,
  config file on native). There is no sync. Conflict resolution is N/A (single writer); see Unmet
  Contracts.
- **Interruption (backgrounding, reload, crash):** every edit dispatches a command immediately;
  snapshots are written by `save_session` on exit and the app requests an autosave every 3 s
  (`auto_save_interval` override) instead of the eframe default 30 s. That is a bounded-loss
  improvement, not a guarantee: edits made in the final seconds before a crash or power failure
  can still be lost, and on WASM the write depends on the browser honoring the interval. On WASM,
  localStorage persists across reloads. Text input is preserved per keystroke, never discarded on
  navigation or mode switch (mobile ↔ desktop share the same `core` state).
- **Undo/recovery:** inline, non-blocking. egui's TextEdit provides in-widget Ctrl+Z/Ctrl+Y for
  the focused field; the calculator has history navigation (arrow keys) and the new Clear-history
  action is the only destructive operation besides typing. There is deliberately no blocking modal
  anywhere in the app (verified: no `Modal` other than the command palette, which is non-blocking
  for state — it is a navigation surface).
- **Validation failures** (URL/email/color) render inline next to the offending field; they never
  discard the entered text.

## Test and gate evidence

- `cargo test -p core` (18 tests), `-p ui` (10 tests), `-p calculator-engine` (9 tests) — all
  green; new tests cover `CalculatorClearHistory` semantics, the 6-entry rail cap, palette
  entry/filter behavior, deterministic tile layout, and the viewport-based card column rule.
- `cargo clippy --workspace --all-targets` — 0 warnings.
- Interactive smoke test against the live native app (egui MCP inspection + native screen
  capture): verified the desktop rail (6 entries, grouped Portfolio/Tools), the identity anchor,
  the Action Hub sheets (About → Edit portfolio opened the editor; Calculator → Clear history
  removed the live transcript while keeping input and definitions), the command palette (Search →
  type "calc" → Enter opened the Calculator window and closed the palette, after fixing Enter
  being consumed by the singleline TextEdit), tile mode (2 open windows became fixed
  non-overlapping 638×838 panes, toggle flipped to "Untile"), and the < 700 px mobile shell.
- **Pixel-verified** via native screen capture of the app window (600×832 native / 800-point
  viewport on mobile, 1280×832 native on desktop): mobile renders 2 card columns with clean
  wrapping (About/Projects row 1, Contact row 2; Calculator/Text analyzer row 1, Color converter
  row 2) and no card overflows the window edge; desktop renders the left rail plus 3 card columns
  that end exactly at the content edge. The wrap is enforced with an explicit `ui.end_row()` after
  every `columns`-th card — egui's `Grid` does not wrap rows implicitly in nested scroll contexts,
  which had left a third card clipped at the screen edge on the compact surface.
- Full gate `env -u NO_COLOR ./check.sh` — see result below (final gate run at the end of this
  audit).

## Changes made

- `crates/core`: new `Command::CalculatorClearHistory` + `CalculatorState::clear_history` (keeps
  input/definitions) + test.
- `crates/ui/src/app.rs`:
  - Six-entry domain rail (desktop left panel) and mobile bottom thumb-zone nav; fixed identity
    anchor (brand → Home); command palette (Modal, Ctrl/Cmd+K) with Enter/arrow handling ordered
    before the input widget so the TextEdit cannot consume them; Action Hub sheets per workspace.
  - Explicit tile/split-pane mode (`TilePanes`, fixed grid, `reset_areas` on untiling).
  - Card grid columns derived from the app viewport via the pure `card_columns` rule (3 ≥ 700 px,
    2 below); rows wrapped with explicit `ui.end_row()` (egui `Grid` does not wrap implicitly in
    scroll contexts); mobile page width clamped to the panel viewport and given a unique scroll id.
  - Window default positions moved right of the rail (they previously overlapped it).
  - `auto_save_interval` overridden to 3 s: snapshot loss on crash is bounded, not 30 s.
- `AGENTS.md`, docs: navigation model and local-only contract documented; unmet contracts listed
  explicitly.