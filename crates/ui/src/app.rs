//! Portfolio application: state, persistence, rendering, and pure tool logic.
//!
//! The app is a client-side personal portfolio with editable browser-local
//! content, three portfolio workspaces, and three tool workspaces. The
//! navigation model follows a fixed taxonomy: a six-domain rail (n ≤ 6) with
//! the portfolio editor exposed only as an object-level action, fixed global
//! anchors (identity brand, search/command palette), deterministic adaptation
//! driven solely by viewport geometry (desktop windows below 700 logical
//! pixels become one full-page mobile view with a bottom navigation bar), and
//! an explicit user-invoked tile mode that splits open workspaces into
//! non-overlapping panes.

use crate::{load_session, save_session};

/// Domain state plus transient session UI state.
///
/// Persistence flows through [`load_session`]/[`save_session`]: the
/// framework-independent snapshot is adapted to and from `eframe::Storage`,
/// with schema migration applied on load.
#[derive(Default)]
pub struct PortfolioApp {
    core: core::CoreState,
    // Transient per-session workspace state. Not persisted: every session
    // starts on the Home view with every desktop window closed.
    workspace: WorkspaceState,
}

/// Destinations available from the menus. Home is the always-visible launcher
/// and is not a desktop window.
type View = core::View;

/// Teal accent for dark theme (#2DD4BF).
const ACCENT_DARK: egui::Color32 = egui::Color32::from_rgb(45, 212, 191);
/// Teal accent for light theme (#0D9488).
const ACCENT_LIGHT: egui::Color32 = egui::Color32::from_rgb(13, 148, 136);

/// A Home launcher destination with its card copy.
struct Destination {
    view: View,
    title: &'static str,
    description: &'static str,
}

const PORTFOLIO_DESTINATIONS: [Destination; 3] = [
    Destination {
        view: View::About,
        title: "About",
        description: "Who I am and what I do.",
    },
    Destination {
        view: View::Projects,
        title: "Projects",
        description: "Selected work and experiments.",
    },
    Destination {
        view: View::Contact,
        title: "Contact",
        description: "Email, website, and social links.",
    },
];

const TOOL_DESTINATIONS: [Destination; 3] = [
    Destination {
        view: View::Calculator,
        title: "Calculator",
        description: "Scientific REPL with units and exact results.",
    },
    Destination {
        view: View::TextAnalyzer,
        title: "Text analyzer",
        description: "Live character, word, and line counts.",
    },
    Destination {
        view: View::ColorConverter,
        title: "Color converter",
        description: "Hex and RGB with a live preview.",
    },
];

/// Portfolio workspaces in the Domain Rail: exactly three read-only
/// destinations. Editing portfolio content is an object-level action surfaced
/// through the workspace Action Hub, never a rail entry, so the rail stays at
/// its n ≤ 6 display cap.
const PORTFOLIO_MENU_ITEMS: [(&str, View); 3] = [
    ("About", View::About),
    ("Projects", View::Projects),
    ("Contact", View::Contact),
];

/// Tool workspaces in the Domain Rail: exactly three destinations.
const TOOL_MENU_ITEMS: [(&str, View); 3] = [
    ("Calculator", View::Calculator),
    ("Text analyzer", View::TextAnalyzer),
    ("Color converter", View::ColorConverter),
];

/// Command-palette targets: navigation destinations plus app-level commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaletteTarget {
    Navigate(View),
    ToggleTile,
    Quit,
}

/// One command-palette entry with its fixed label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PaletteEntry {
    label: &'static str,
    target: PaletteTarget,
}

/// Object-level actions a workspace can expose through its Action Hub sheet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HubAction {
    EditPortfolio,
    AddProject,
    ClearHistory,
}

/// An Action Hub sheet entry: action label plus the action itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HubEntry {
    label: &'static str,
    action: HubAction,
}

/// The About workspace Action Hub: edit the shared portfolio object.
const ABOUT_HUB: [HubEntry; 1] = [HubEntry {
    label: "Edit portfolio",
    action: HubAction::EditPortfolio,
}];

/// The Projects workspace Action Hub: create a project object or edit the
/// portfolio object that owns it. Adding is non-destructive (the new project
/// is empty and editable).
const PROJECTS_HUB: [HubEntry; 2] = [
    HubEntry {
        label: "Add project",
        action: HubAction::AddProject,
    },
    HubEntry {
        label: "Edit portfolio",
        action: HubAction::EditPortfolio,
    },
];

/// The Contact workspace Action Hub: edit the shared portfolio object.
const CONTACT_HUB: [HubEntry; 1] = [HubEntry {
    label: "Edit portfolio",
    action: HubAction::EditPortfolio,
}];

/// The Calculator workspace Action Hub: clear the transcript. Non-destructive
/// to persisted state — variables and definitions survive the clear.
const CALCULATOR_HUB: [HubEntry; 1] = [HubEntry {
    label: "Clear history",
    action: HubAction::ClearHistory,
}];

/// The command-palette entry list: Home, the six rail workspaces, the
/// portfolio-edit action, the tile-mode toggle (desktop only), and (native
/// only) Quit. Ordering is stable so palette results never jump between
/// keystrokes.
#[must_use]
fn palette_entries(tiled: bool, native: bool, desktop: bool) -> Vec<PaletteEntry> {
    let mut entries = Vec::with_capacity(10);
    entries.push(PaletteEntry {
        label: "Home",
        target: PaletteTarget::Navigate(View::Home),
    });
    for (label, view) in PORTFOLIO_MENU_ITEMS.iter().chain(TOOL_MENU_ITEMS.iter()) {
        entries.push(PaletteEntry {
            label,
            target: PaletteTarget::Navigate(*view),
        });
    }
    entries.push(PaletteEntry {
        label: "Edit portfolio",
        target: PaletteTarget::Navigate(View::EditPortfolio),
    });
    if desktop {
        entries.push(PaletteEntry {
            label: if tiled {
                "Untile windows"
            } else {
                "Tile windows"
            },
            target: PaletteTarget::ToggleTile,
        });
    }
    if native {
        entries.push(PaletteEntry {
            label: "Quit",
            target: PaletteTarget::Quit,
        });
    }
    entries
}

/// Case-insensitive substring filter over palette entries. An empty query
/// keeps every entry; ordering is preserved (stable navigation).
#[must_use]
fn palette_filter(query: &str, entries: &[PaletteEntry]) -> Vec<PaletteEntry> {
    let query = query.to_lowercase();
    entries
        .iter()
        .filter(|entry| query.is_empty() || entry.label.to_lowercase().contains(&query))
        .copied()
        .collect()
}

/// Deterministic pane grid over `rect` for `count` equal, non-overlapping
/// panes separated by `gap` logical points. Panes fill left-to-right,
/// top-to-bottom so positions never depend on window content or history.
#[must_use]
fn tile_layout(rect: egui::Rect, count: usize, gap: f32) -> Vec<egui::Rect> {
    if count == 0 {
        return Vec::new();
    }
    let mut columns = 1;
    while columns * columns < count {
        columns += 1;
    }
    let rows = count.div_ceil(columns);
    let cell_width = (rect.width() - gap * (columns as f32 - 1.0)) / columns as f32;
    let cell_height = (rect.height() - gap * (rows as f32 - 1.0)) / rows as f32;
    let mut panes = Vec::with_capacity(count);
    for index in 0..count {
        let column = index % columns;
        let row = index / columns;
        let min = egui::pos2(
            rect.min.x + column as f32 * (cell_width + gap),
            rect.min.y + row as f32 * (cell_height + gap),
        );
        panes.push(egui::Rect::from_min_size(
            min,
            egui::vec2(cell_width, cell_height),
        ));
    }
    panes
}

/// The teal accent for the given theme mode.
fn accent(dark: bool) -> egui::Color32 {
    if dark { ACCENT_DARK } else { ACCENT_LIGHT }
}

/// The accent color for the current (resolved) theme.
fn accent_color(ui: &egui::Ui) -> egui::Color32 {
    accent(ui.style().visuals.dark_mode)
}

/// Apply the app-wide visual style to both dark and light themes: a teal
/// accent, rounded corners, and a consistent spacing scale. Runs each frame so
/// the System/Dark/Light theme buttons keep working.
fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        let accent = accent(style.visuals.dark_mode);

        style.visuals.hyperlink_color = accent;
        style.visuals.selection.bg_fill = accent.gamma_multiply(0.25);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, accent);
        style.visuals.window_corner_radius = 10.0.into();
        style.visuals.menu_corner_radius = 8.0.into();
        for widget in [
            &mut style.visuals.widgets.noninteractive,
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            widget.corner_radius = 6.0.into();
        }

        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
    });
}

type PortfolioContent = core::Portfolio;
type Project = core::Project;

/// Transient UI state. Not persisted: every session starts on the Home view
/// with every desktop window closed and no validation errors.
struct WorkspaceState {
    mobile_view: View,
    about_open: bool,
    projects_open: bool,
    contact_open: bool,
    edit_open: bool,
    calculator_open: bool,
    text_analyzer_open: bool,
    color_converter_open: bool,
    color_error: Option<String>,
    /// Explicit user-invoked split mode: open workspaces as non-overlapping
    /// panes instead of free-floating windows. Never the default state.
    tiled: bool,
    palette_open: bool,
    palette_query: String,
    palette_selection: usize,
    palette_focus_requested: bool,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            mobile_view: View::Home,
            about_open: false,
            projects_open: false,
            contact_open: false,
            edit_open: false,
            calculator_open: false,
            text_analyzer_open: false,
            color_converter_open: false,
            color_error: None,
            tiled: false,
            palette_open: false,
            palette_query: String::new(),
            palette_selection: 0,
            palette_focus_requested: false,
        }
    }
}

impl WorkspaceState {
    /// Set the desktop open flag for the given view, and route to it on mobile.
    fn activate_view(&mut self, view: View, mobile: bool) {
        match view {
            View::Home => {}
            View::About => self.about_open = true,
            View::Projects => self.projects_open = true,
            View::Contact => self.contact_open = true,
            View::EditPortfolio => self.edit_open = true,
            View::Calculator => self.calculator_open = true,
            View::TextAnalyzer => self.text_analyzer_open = true,
            View::ColorConverter => self.color_converter_open = true,
        }
        if mobile {
            self.mobile_view = view;
        }
    }
}

impl PortfolioApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        match cc.storage {
            Some(storage) => Self {
                core: core::CoreState::from_snapshot(load_session(storage)),
                workspace: WorkspaceState::default(),
            },
            None => Self::default(),
        }
    }
    /// True when the viewport is too narrow for simultaneous windows.
    fn is_mobile(ctx: &egui::Context) -> bool {
        ctx.content_rect().width() < 700.0
    }

    /// Route to a destination: dispatch navigation and update the workspace
    /// open flags (and the mobile destination when `mobile` is set).
    fn navigate(&mut self, view: View, mobile: bool) {
        self.core
            .dispatch(core::Command::Navigate(view))
            .expect("navigation command must succeed");
        self.workspace.activate_view(view, mobile);
    }

    /// Open the command palette, resetting query and selection.
    fn open_palette(&mut self) {
        self.workspace.palette_open = true;
        self.workspace.palette_query.clear();
        self.workspace.palette_selection = 0;
        self.workspace.palette_focus_requested = true;
    }

    /// Close the command palette, discarding the query.
    fn close_palette(&mut self) {
        self.workspace.palette_open = false;
        self.workspace.palette_query.clear();
        self.workspace.palette_selection = 0;
    }

    /// Toggle the explicit desktop split mode. Untiling restores floating
    /// windows to their configured defaults.
    fn toggle_tile(&mut self, ctx: &egui::Context) {
        self.workspace.tiled = !self.workspace.tiled;
        if !self.workspace.tiled {
            ctx.memory_mut(|memory| memory.reset_areas());
        }
    }

    /// Run a palette target. `mobile` selects between routing to the mobile
    /// destination and opening a desktop window.
    fn activate_palette_target(
        &mut self,
        target: PaletteTarget,
        mobile: bool,
        ctx: &egui::Context,
    ) {
        match target {
            PaletteTarget::Navigate(view) => self.navigate(view, mobile),
            PaletteTarget::ToggleTile => self.toggle_tile(ctx),
            PaletteTarget::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    /// Run an Action Hub action for the given rendering mode.
    fn apply_hub_action(&mut self, action: HubAction, mobile: bool) {
        match action {
            HubAction::EditPortfolio => self.navigate(View::EditPortfolio, mobile),
            HubAction::AddProject => self
                .core
                .dispatch(core::Command::AddProject)
                .expect("add project command must succeed"),
            HubAction::ClearHistory => self
                .core
                .dispatch(core::Command::CalculatorClearHistory)
                .expect("clear history command must succeed"),
        }
    }

    /// The identity anchor: the portfolio display name at a fixed position on
    /// every surface. Clicking it returns to the Home launcher.
    fn show_brand(&mut self, ui: &mut egui::Ui, mobile: bool) {
        let name = if self.core.portfolio.display_name.is_empty() {
            "Portfolio"
        } else {
            &self.core.portfolio.display_name
        };
        let brand = egui::Button::new(egui::RichText::new(name).strong()).frame(false);
        if ui.add(brand).on_hover_text("Home").clicked() {
            self.navigate(View::Home, mobile);
        }
    }

    /// The command palette: a centered modal with a type-ahead filter over all
    /// destinations and app-level commands (the search/command global anchor).
    fn show_palette(&mut self, ui: &egui::Ui, mobile: bool) {
        use egui::containers::Modal;

        let input_id = egui::Id::new("command_palette_input");
        if self.workspace.palette_focus_requested {
            ui.memory_mut(|memory| memory.request_focus(input_id));
            self.workspace.palette_focus_requested = false;
        }

        let entries = palette_entries(
            self.workspace.tiled,
            cfg!(not(target_arch = "wasm32")),
            !mobile,
        );
        let mut query = self.workspace.palette_query.clone();
        let mut activated = None;
        let mut should_close = false;

        let modal = Modal::new(egui::Id::new("command_palette")).show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            // Handle Enter/arrows before the text edit is added: a singleline
            // TextEdit consumes Enter (submit) and arrow keys (cursor) itself,
            // which would starve the palette's selection handling.
            if ui.memory(|memory| memory.has_focus(input_id)) {
                let arrow_down = ui.input_mut(|input| {
                    input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)
                });
                let arrow_up = ui.input_mut(|input| {
                    input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
                });
                let enter = ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                let filtered = palette_filter(&self.workspace.palette_query, &entries);
                let count = filtered.len();
                if count > 0 {
                    self.workspace.palette_selection =
                        self.workspace.palette_selection.min(count - 1);
                    if arrow_down {
                        self.workspace.palette_selection =
                            (self.workspace.palette_selection + 1) % count;
                    }
                    if arrow_up {
                        self.workspace.palette_selection =
                            (self.workspace.palette_selection + count - 1) % count;
                    }
                    if enter {
                        activated = Some(filtered[self.workspace.palette_selection].target);
                        should_close = true;
                    }
                }
            }
            let response = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .id(input_id)
                    .desired_width(f32::INFINITY)
                    .hint_text("Type a workspace or action…"),
            );
            if response.changed() {
                self.workspace.palette_query = query.clone();
                self.workspace.palette_selection = 0;
            }
            let filtered = palette_filter(&self.workspace.palette_query, &entries);
            let count = filtered.len();
            if count > 0 {
                self.workspace.palette_selection = self.workspace.palette_selection.min(count - 1);
            }
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (index, entry) in filtered.iter().enumerate() {
                        if ui
                            .selectable_label(
                                index == self.workspace.palette_selection,
                                entry.label,
                            )
                            .clicked()
                        {
                            activated = Some(entry.target);
                            should_close = true;
                        }
                    }
                    if count == 0 {
                        ui.label("No matches.");
                    }
                });
        });
        if modal.should_close() {
            should_close = true;
        }
        if should_close {
            self.close_palette();
        }
        if let Some(target) = activated {
            self.activate_palette_target(target, mobile, ui.ctx());
        }
    }
}

impl eframe::App for PortfolioApp {
    /// We manage persistence ourselves in [`Self::save`]; egui's own memory
    /// (window geometry, collapsed state) must not be restored across sessions.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        save_session(storage, &self.core.snapshot());
    }

    /// Keep the interruption loss window small: both integrations also call
    /// [`Self::save`] on this interval, so a crash loses at most a few seconds
    /// of edits instead of the 30-second default.
    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(3)
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        apply_style(ui.ctx());
        let mobile = Self::is_mobile(ui.ctx());
        // The command palette is the keyboard path to every destination.
        if !self.workspace.palette_open {
            let shortcut =
                ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::K));
            if shortcut {
                self.open_palette();
            }
        }
        if mobile {
            self.show_mobile(ui);
        } else {
            self.show_desktop(ui);
        }
        if self.workspace.palette_open {
            self.show_palette(ui, mobile);
        }
    }
}

impl PortfolioApp {
    /// Desktop layout: top anchor bar (identity, command, tile, theme), the
    /// left domain rail, the Home launcher in the central panel, and one
    /// window per open workspace — free-floating by default, tiled into
    /// non-overlapping panes when the user invokes split mode.
    #[expect(
        clippy::too_many_lines,
        reason = "desktop shell keeps its anchor bar, rail, and all seven workspace windows together"
    )]
    fn show_desktop(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_anchors").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ui.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(8.0);
                }

                // Global anchors: identity fixed at the left, command/search,
                // split mode, and theme grouped at the fixed right cluster.
                self.show_brand(ui, false);
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::widgets::global_theme_preference_buttons(ui);
                    ui.add_space(8.0);
                    let tile_label = if self.workspace.tiled {
                        "Untile"
                    } else {
                        "Tile"
                    };
                    if ui
                        .button(tile_label)
                        .on_hover_text("Split open workspaces into non-overlapping panes")
                        .clicked()
                    {
                        self.toggle_tile(ui.ctx());
                    }
                    ui.add_space(8.0);
                    if ui
                        .button("Search")
                        .on_hover_text("Open command palette (Ctrl/Cmd+K)")
                        .clicked()
                    {
                        self.open_palette();
                    }
                });
            });
        });

        self.show_rail(ui);

        let frame = egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::same(28));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            // Central launcher:
            if let Some(view) = show_home(ui, &self.core.portfolio) {
                self.navigate(view, false);
            }

            // All other mutable state must be borrowed disjointly from
            // `self.workspace` so `.open(&mut flag)` and the window closures
            // can both touch different fields.
            let Self {
                core, workspace, ..
            } = self;

            // Action Hub activations are collected while the windows render
            // and applied afterwards, when the disjoint borrows have ended.
            let mut hub_action = None;

            let panes = if workspace.tiled {
                tile_layout(ui.max_rect(), open_window_count(workspace), 4.0)
            } else {
                Vec::new()
            };
            let mut tile = workspace.tiled.then_some(TilePanes {
                panes: &panes,
                next: 0,
            });

            window_or_tile(
                tile.as_mut(),
                &mut workspace.about_open,
                "About",
                [240.0, 90.0],
                [420.0, 300.0],
                ui.ctx(),
                |ui| {
                    if let Some(action) = workspace_header(ui, &ABOUT_HUB) {
                        hub_action = Some(action);
                    }
                    show_about(ui, &core.portfolio);
                },
            );

            window_or_tile(
                tile.as_mut(),
                &mut workspace.projects_open,
                "Projects",
                [300.0, 150.0],
                [520.0, 420.0],
                ui.ctx(),
                |ui| {
                    if let Some(action) = workspace_header(ui, &PROJECTS_HUB) {
                        hub_action = Some(action);
                    }
                    show_projects(ui, &core.portfolio);
                },
            );

            window_or_tile(
                tile.as_mut(),
                &mut workspace.contact_open,
                "Contact",
                [360.0, 210.0],
                [420.0, 280.0],
                ui.ctx(),
                |ui| {
                    if let Some(action) = workspace_header(ui, &CONTACT_HUB) {
                        hub_action = Some(action);
                    }
                    show_contact(ui, &core.portfolio);
                },
            );

            window_or_tile(
                tile.as_mut(),
                &mut workspace.edit_open,
                "Edit portfolio",
                [280.0, 90.0],
                [620.0, 560.0],
                ui.ctx(),
                |ui| show_portfolio_editor(ui, core),
            );

            window_or_tile(
                tile.as_mut(),
                &mut workspace.calculator_open,
                "Calculator",
                [340.0, 90.0],
                [360.0, 300.0],
                ui.ctx(),
                |ui| {
                    if let Some(action) = workspace_header(ui, &CALCULATOR_HUB) {
                        hub_action = Some(action);
                    }
                    show_calculator(ui, core);
                },
            );

            window_or_tile(
                tile.as_mut(),
                &mut workspace.text_analyzer_open,
                "Text analyzer",
                [400.0, 150.0],
                [520.0, 420.0],
                ui.ctx(),
                |ui| show_text_analyzer(ui, core),
            );

            window_or_tile(
                tile.as_mut(),
                &mut workspace.color_converter_open,
                "Color converter",
                [460.0, 210.0],
                [420.0, 380.0],
                ui.ctx(),
                |ui| show_color_converter(ui, core, &mut workspace.color_error),
            );

            if let Some(action) = hub_action {
                self.apply_hub_action(action, false);
            }
        });
    }

    /// The Domain Rail: a fixed left panel listing the six workspaces grouped
    /// by the Portfolio and Tools taxonomies, at the n ≤ 6 visible cap.
    fn show_rail(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("domain_rail")
            .exact_size(176.0)
            .resizable(false)
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 12)))
            .show(ui, |ui| {
                ui.add_space(2.0);
                for (section, items) in [
                    ("Portfolio", PORTFOLIO_MENU_ITEMS.as_slice()),
                    ("Tools", TOOL_MENU_ITEMS.as_slice()),
                ] {
                    section_label(ui, section, accent_color(ui));
                    ui.add_space(4.0);
                    for (label, view) in items {
                        let active = self.core.active_view == *view;
                        if ui.selectable_label(active, *label).clicked() {
                            self.navigate(*view, false);
                        }
                    }
                    ui.add_space(12.0);
                }
            });
    }

    /// Mobile layout: one full-page workspace at a time. The identity anchor
    /// stays at the fixed top-left position; the rail groups live in the
    /// bottom thumb-zone bar as transient sheets with ≤ 3 choices each.
    fn show_mobile(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("mobile_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                self.show_brand(ui, true);
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::widgets::global_theme_preference_buttons(ui);
                });
            });
        });

        // Bottom thumb-zone navigation: two group sheets over the six-domain
        // rail. Every decision point here presents at most 3 co-equal choices.
        egui::Panel::bottom("mobile_nav").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal_centered(|ui| {
                let portfolio_button = ui.button("Portfolio");
                let tools_button = ui.button("Tools");
                let search_button = ui.button("Search");
                egui::containers::Popup::menu(&portfolio_button).show(|ui| {
                    for (label, view) in PORTFOLIO_MENU_ITEMS {
                        if ui.button(label).clicked() {
                            self.navigate(view, true);
                            ui.close();
                        }
                    }
                });
                egui::containers::Popup::menu(&tools_button).show(|ui| {
                    for (label, view) in TOOL_MENU_ITEMS {
                        if ui.button(label).clicked() {
                            self.navigate(view, true);
                            ui.close();
                        }
                    }
                });
                if search_button.clicked() {
                    self.open_palette();
                }
            });
        });

        let frame = egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::same(16));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            // The page width comes from the panel (viewport), not from the
            // scroll area's inner Ui: a vertical-only scroll area must never
            // grow horizontally, or card grids overflow the screen edge.
            let page_width = ui.available_width();
            egui::ScrollArea::vertical()
                .id_salt("mobile_page")
                .show(ui, |ui| {
                    ui.set_width(page_width);
                    let launch = match self.workspace.mobile_view {
                        View::Home => show_home(ui, &self.core.portfolio),
                        View::About => {
                            if let Some(action) = workspace_header(ui, &ABOUT_HUB) {
                                self.apply_hub_action(action, true);
                            }
                            show_about(ui, &self.core.portfolio);
                            None
                        }
                        View::Projects => {
                            if let Some(action) = workspace_header(ui, &PROJECTS_HUB) {
                                self.apply_hub_action(action, true);
                            }
                            show_projects(ui, &self.core.portfolio);
                            None
                        }
                        View::Contact => {
                            if let Some(action) = workspace_header(ui, &CONTACT_HUB) {
                                self.apply_hub_action(action, true);
                            }
                            show_contact(ui, &self.core.portfolio);
                            None
                        }
                        View::EditPortfolio => {
                            show_portfolio_editor(ui, &mut self.core);
                            None
                        }
                        View::Calculator => {
                            if let Some(action) = workspace_header(ui, &CALCULATOR_HUB) {
                                self.apply_hub_action(action, true);
                            }
                            show_calculator(ui, &mut self.core);
                            None
                        }
                        View::TextAnalyzer => {
                            show_text_analyzer(ui, &mut self.core);
                            None
                        }
                        View::ColorConverter => {
                            show_color_converter(
                                ui,
                                &mut self.core,
                                &mut self.workspace.color_error,
                            );
                            None
                        }
                    };
                    if let Some(view) = launch {
                        self.navigate(view, true);
                    }
                });
        });
    }
}

/// Render an open `egui::Window` with stable position/size defaults for the
/// given destination. The `open` flag is bound to the title-bar close button,
/// so closing the window flips the caller's state and the window stays closed.
fn window(
    open: &mut bool,
    title: &'static str,
    default_pos: [f32; 2],
    default_size: [f32; 2],
    ctx: &egui::Context,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    if !*open {
        return;
    }
    let mut builder = egui::Window::new(title)
        .open(open)
        .default_pos(default_pos)
        .default_size(default_size)
        .collapsible(false)
        .resizable(true)
        .vscroll(true);
    builder = builder.constrain(true);
    builder.show(ctx, add_contents);
}

/// Tile state shared by the open desktop workspaces: the pane grid and the
/// next unassigned pane index.
struct TilePanes<'a> {
    panes: &'a [egui::Rect],
    next: usize,
}

impl TilePanes<'_> {
    /// The pane for the next open workspace, if split mode is active.
    fn take(&mut self) -> Option<egui::Rect> {
        let pane = self.panes.get(self.next).copied();
        if pane.is_some() {
            self.next += 1;
        }
        pane
    }
}

/// Render one workspace window, either as a free-floating window or, in the
/// user-invoked split mode, as a fixed non-overlapping pane. Panes are
/// assigned in the fixed window order (About, Projects, Contact, Editor,
/// Calculator, Text analyzer, Color converter), so pane positions never
/// depend on which workspace was opened last.
fn window_or_tile(
    tile: Option<&mut TilePanes<'_>>,
    open: &mut bool,
    title: &'static str,
    default_pos: [f32; 2],
    default_size: [f32; 2],
    ctx: &egui::Context,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    if !*open {
        return;
    }
    if let Some(pane) = tile.and_then(TilePanes::take) {
        egui::Window::new(title)
            .open(open)
            .fixed_pos(pane.min)
            .fixed_size(pane.size())
            .collapsible(false)
            .resizable(false)
            .vscroll(true)
            .show(ctx, add_contents);
        return;
    }
    window(open, title, default_pos, default_size, ctx, add_contents);
}

/// The Action Hub: an "Actions" button that opens a sheet of object-level
/// actions for the current workspace. Returns the activated action, if any.
fn action_hub(ui: &mut egui::Ui, entries: &[HubEntry]) -> Option<HubAction> {
    if entries.is_empty() {
        return None;
    }
    let button = ui.button("Actions");
    let mut activated = None;
    egui::containers::Popup::menu(&button).show(|ui| {
        for entry in entries {
            if ui.button(entry.label).clicked() {
                activated = Some(entry.action);
                ui.close();
            }
        }
    });
    activated
}

/// A workspace header row: the object-level actions of the workspace surface
/// as a trailing Action Hub button (top-right of the workspace surface).
fn workspace_header(ui: &mut egui::Ui, entries: &[HubEntry]) -> Option<HubAction> {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            action_hub(ui, entries)
        })
        .inner
    })
    .inner
}

/// Number of currently open desktop workspaces.
#[must_use]
fn open_window_count(workspace: &WorkspaceState) -> usize {
    [
        workspace.about_open,
        workspace.projects_open,
        workspace.contact_open,
        workspace.edit_open,
        workspace.calculator_open,
        workspace.text_analyzer_open,
        workspace.color_converter_open,
    ]
    .into_iter()
    .filter(|open| *open)
    .count()
}

/// The Home launcher: hero name/headline, then card grids for portfolio views
/// and tools. Returns the clicked destination, if any.
fn show_home(ui: &mut egui::Ui, content: &PortfolioContent) -> Option<View> {
    let accent = accent_color(ui);

    let name = if content.display_name.is_empty() {
        "Personal Portfolio"
    } else {
        &content.display_name
    };
    ui.label(egui::RichText::new(name).size(32.0).strong());
    ui.add_space(6.0);

    let headline = if content.headline.is_empty() {
        "Client-side Rust portfolio and interactive tools."
    } else {
        &content.headline
    };
    ui.label(
        egui::RichText::new(headline)
            .size(15.0)
            .color(ui.visuals().weak_text_color()),
    );

    ui.add_space(24.0);
    section_label(ui, "Portfolio", accent);
    ui.add_space(10.0);
    let mut clicked = None;
    destination_grid(ui, "portfolio_cards", &PORTFOLIO_DESTINATIONS, &mut clicked);

    ui.add_space(28.0);
    section_label(ui, "Tools", accent);
    ui.add_space(10.0);
    destination_grid(ui, "tools_cards", &TOOL_DESTINATIONS, &mut clicked);

    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.label(
            egui::RichText::new("Built with Rust · egui · eframe")
                .size(12.0)
                .weak(),
        );
    });

    clicked
}

/// A small, uppercase-styled section heading.
fn section_label(ui: &mut egui::Ui, text: &str, accent: egui::Color32) {
    ui.label(egui::RichText::new(text).size(12.0).strong().color(accent));
}

/// Destination card columns for a given viewport width: 3 on wide surfaces,
/// 2 in the compact (mobile) layout. Pure so the geometry adaptation is
/// testable; the viewport is the only observable parameter the adaptation
/// may depend on.
#[must_use]
fn card_columns(viewport_width: f32) -> usize {
    if viewport_width >= 700.0 { 3 } else { 2 }
}

/// Lay the given destinations out as an equal-width card grid. Column count
/// follows the viewport geometry; the card row is additionally clamped to
/// the viewport so a vertical scroll surface never grows horizontally.
fn destination_grid(
    ui: &mut egui::Ui,
    grid_id: &'static str,
    destinations: &[Destination],
    clicked: &mut Option<View>,
) {
    let viewport_width = ui.ctx().content_rect().width();
    let columns = card_columns(viewport_width);
    let gap = ui.spacing().item_spacing.x;
    let card_w = ((ui.available_width().min(viewport_width) - (columns as f32 - 1.0) * gap)
        / columns as f32)
        .max(150.0);
    let card_h = 76.0;

    egui::Grid::new(grid_id)
        .num_columns(columns)
        .spacing(egui::vec2(gap, gap))
        .show(ui, |ui| {
            for (index, destination) in destinations.iter().enumerate() {
                if destination_card(ui, destination, egui::vec2(card_w, card_h)) {
                    *clicked = Some(destination.view);
                }
                // Grid wraps rows only on an explicit `end_row`; relying on
                // implicit wrapping lets the row grow past the viewport.
                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
}

/// A clickable card: a large rounded button with a bold title and a wrapped
/// description.
fn destination_card(ui: &mut egui::Ui, destination: &Destination, size: egui::Vec2) -> bool {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        destination.title,
        0.0,
        egui::text::TextFormat {
            font_id: egui::FontId::proportional(15.0),
            color: ui.visuals().strong_text_color(),
            ..Default::default()
        },
    );
    job.append("\n", 0.0, egui::text::TextFormat::default());
    job.append(
        destination.description,
        0.0,
        egui::text::TextFormat {
            font_id: egui::FontId::proportional(12.5),
            color: ui.visuals().text_color(),
            ..Default::default()
        },
    );

    let button = egui::Button::new(job)
        .min_size(size)
        .corner_radius(8.0)
        .wrap_mode(egui::TextWrapMode::Wrap);
    ui.add(button).clicked()
}

fn show_about(ui: &mut egui::Ui, content: &PortfolioContent) {
    if content.about.is_empty() {
        ui.label("Use Edit portfolio to add an introduction.");
    } else {
        ui.label(&content.about);
    }
}

fn show_projects(ui: &mut egui::Ui, content: &PortfolioContent) {
    let visible: Vec<&Project> = content
        .projects
        .iter()
        .filter(|p| !(p.title.is_empty() && p.summary.is_empty() && p.url.is_empty()))
        .collect();
    if visible.is_empty() {
        ui.label("No projects have been added yet.");
        return;
    }
    for project in visible {
        ui.add_space(6.0);
        let title = if project.title.is_empty() {
            "Untitled project"
        } else {
            &project.title
        };
        ui.strong(title);
        if !project.summary.is_empty() {
            ui.label(&project.summary);
        }
        if !project.url.is_empty() {
            link_or_plain(ui, &project.url);
        }
        ui.separator();
    }
}

fn show_contact(ui: &mut egui::Ui, content: &PortfolioContent) {
    let mut anything = false;
    ui.add_space(6.0);
    if !content.email.is_empty() {
        anything = true;
        if core::is_valid_email(&content.email) {
            ui.hyperlink_to(&content.email, format!("mailto:{}", content.email));
        } else {
            ui.label(&content.email);
        }
    }
    if !content.website.is_empty() {
        anything = true;
        link_or_plain(ui, &content.website);
    }
    if !content.github.is_empty() {
        anything = true;
        link_or_plain(ui, &content.github);
    }
    if !anything {
        ui.label("No contact links have been added yet.");
    }
}

/// Render a URL as a hyperlink when it passes `core::is_valid_url`,
/// otherwise as plain text.
fn link_or_plain(ui: &mut egui::Ui, url: &str) {
    if core::is_valid_url(url) {
        ui.hyperlink_to(url, url);
    } else {
        ui.label(url);
    }
}

/// Show the portfolio link-format error for one non-empty invalid URL.
fn invalid_url_message(ui: &mut egui::Ui, url: &str) {
    if !url.is_empty() && !core::is_valid_url(url) {
        ui.colored_label(
            egui::Color32::RED,
            "Links must start with http:// or https://.",
        );
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "portfolio editor keeps its fields and project controls together"
)]
fn show_portfolio_editor(ui: &mut egui::Ui, state: &mut core::CoreState) {
    let content = state.portfolio.clone();
    let mut commands = Vec::new();
    ui.label("Edits are stored only in this browser.");
    ui.add_space(8.0);

    text_field(
        ui,
        "Display name",
        &content.display_name,
        false,
        &mut commands,
        |value| core::Command::SetPortfolioField {
            field: core::PortfolioField::DisplayName,
            value,
        },
    );
    text_field(
        ui,
        "Headline",
        &content.headline,
        false,
        &mut commands,
        |value| core::Command::SetPortfolioField {
            field: core::PortfolioField::Headline,
            value,
        },
    );
    text_field(ui, "About", &content.about, true, &mut commands, |value| {
        core::Command::SetPortfolioField {
            field: core::PortfolioField::About,
            value,
        }
    });
    text_field(ui, "Email", &content.email, false, &mut commands, |value| {
        core::Command::SetPortfolioField {
            field: core::PortfolioField::Email,
            value,
        }
    });
    if !content.email.is_empty() && !core::is_valid_email(&content.email) {
        ui.colored_label(egui::Color32::RED, "Enter an email address containing @.");
    }
    text_field(
        ui,
        "Website",
        &content.website,
        false,
        &mut commands,
        |value| core::Command::SetPortfolioField {
            field: core::PortfolioField::Website,
            value,
        },
    );
    invalid_url_message(ui, &content.website);
    text_field(
        ui,
        "GitHub",
        &content.github,
        false,
        &mut commands,
        |value| core::Command::SetPortfolioField {
            field: core::PortfolioField::Github,
            value,
        },
    );
    invalid_url_message(ui, &content.github);

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    ui.heading("Projects");
    if ui.button("Add project").clicked() {
        commands.push(core::Command::AddProject);
    }

    for (index, project) in content.projects.iter().enumerate() {
        ui.add_space(8.0);
        ui.label(format!("Project {}", index + 1));
        text_field(ui, "Title", &project.title, false, &mut commands, |value| {
            core::Command::SetProjectField {
                index,
                field: core::ProjectField::Title,
                value,
            }
        });
        text_field(
            ui,
            "Summary",
            &project.summary,
            true,
            &mut commands,
            |value| core::Command::SetProjectField {
                index,
                field: core::ProjectField::Summary,
                value,
            },
        );
        text_field(ui, "URL", &project.url, false, &mut commands, |value| {
            core::Command::SetProjectField {
                index,
                field: core::ProjectField::Url,
                value,
            }
        });
        invalid_url_message(ui, &project.url);
    }

    for command in commands {
        state
            .dispatch(command)
            .expect("portfolio editor command must target an existing project");
    }
}

/// A labeled text edit that pushes one domain command per change; the caller
/// provides the command constructor so portfolio and project fields share
/// this single editor.
fn text_field(
    ui: &mut egui::Ui,
    label: &str,
    current: &str,
    multiline: bool,
    commands: &mut Vec<core::Command>,
    command: impl Fn(String) -> core::Command,
) {
    ui.label(label);
    let mut value = current.to_owned();
    let changed = if multiline {
        ui.text_edit_multiline(&mut value).changed()
    } else {
        ui.text_edit_singleline(&mut value).changed()
    };
    if changed {
        commands.push(command(value));
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "calculator view keeps history, input, preview, and completion controls together"
)]
fn show_calculator(ui: &mut egui::Ui, state: &mut core::CoreState) {
    ui.heading("Scientific calculator");
    ui.label("Examples: 1/3, sqrt(2), 5 m/s to km/h, x = 2");
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            for entry in state.calculator.history().iter().rev() {
                ui.monospace(&entry.input);
                match &entry.outcome {
                    core::HistoryOutcome::Value {
                        primary,
                        approximation,
                    } => {
                        ui.label(primary);
                        if let Some(approximation) = approximation {
                            ui.label(approximation);
                        }
                    }
                    core::HistoryOutcome::Error { message } => {
                        ui.colored_label(egui::Color32::RED, message);
                    }
                }
                ui.separator();
            }
        });

    let input_id = egui::Id::new("calculator_repl_input");
    if let Some(warning) = state.calculator.restore_warning() {
        ui.colored_label(egui::Color32::YELLOW, warning);
    }
    if ui.memory(|memory| memory.has_focus(input_id)) {
        let up = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
        let down =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
        let tab = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
        if tab && let Some(completion) = state.calculator.completions().first().cloned() {
            state
                .dispatch(core::Command::CalculatorComplete {
                    replacement: completion.replacement,
                    insert: completion.insert,
                })
                .expect("calculator command must succeed");
        }
        if up {
            state
                .dispatch(core::Command::CalculatorHistoryUp)
                .expect("calculator command must succeed");
        }
        if down {
            state
                .dispatch(core::Command::CalculatorHistoryDown)
                .expect("calculator command must succeed");
        }
    }

    let mut input = state.calculator.input().to_owned();
    let response = ui.add(
        egui::TextEdit::singleline(&mut input)
            .id(input_id)
            .hint_text("Enter an expression"),
    );
    if response.changed() {
        state
            .dispatch(core::Command::CalculatorSetInput(input))
            .expect("calculator command must succeed");
    }
    if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        state
            .dispatch(core::Command::CalculatorEvaluate)
            .expect("calculator command must succeed");
        ui.memory_mut(|memory| memory.request_focus(input_id));
    }

    let completions: Vec<calculator_engine::Completion> = state
        .calculator
        .completions()
        .iter()
        .take(8)
        .cloned()
        .collect();
    if !completions.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for completion in completions {
                if ui.small_button(&completion.display).clicked() {
                    state
                        .dispatch(core::Command::CalculatorComplete {
                            replacement: completion.replacement,
                            insert: completion.insert,
                        })
                        .expect("calculator command must succeed");
                    ui.memory_mut(|memory| memory.request_focus(input_id));
                }
            }
        });
    }

    match state.calculator.preview() {
        Ok(Some(value)) => {
            ui.label(format!("Preview: {}", value.primary));
            if let Some(approximation) = &value.approximation {
                ui.label(approximation);
            }
        }
        Err(error) => {
            ui.colored_label(
                egui::Color32::RED,
                format!(
                    "{} (bytes {}..{})",
                    error.message, error.span.start, error.span.end
                ),
            );
        }
        Ok(None) => {}
    }
}

fn show_text_analyzer(ui: &mut egui::Ui, state: &mut core::CoreState) {
    let mut text = state.text_analyzer.text().to_owned();
    let response = ui.add(
        egui::TextEdit::multiline(&mut text)
            .desired_width(f32::INFINITY)
            .desired_rows(10),
    );
    if response.changed() {
        state
            .dispatch(core::Command::TextAnalyzerSetText(text))
            .expect("text analyzer command must succeed");
    }
    let stats = state.text_analyzer.stats();
    ui.label(format!(
        "Characters: {}\nWords: {}\nLines: {}",
        stats.characters, stats.words, stats.lines
    ));
}

fn show_color_converter(
    ui: &mut egui::Ui,
    state: &mut core::CoreState,
    error: &mut Option<String>,
) {
    let mut hex_input = state.color_converter.hex_input().to_owned();
    ui.horizontal(|ui| {
        ui.label("Hex");
        ui.add(egui::TextEdit::singleline(&mut hex_input).hint_text("#RRGGBB"));
        if ui.button("Apply hex").clicked() {
            match state.dispatch(core::Command::ColorApplyHex(hex_input.clone())) {
                Ok(()) => *error = None,
                Err(core::CoreError::Color(color_error)) => {
                    *error = Some(color_error.message().to_owned());
                }
                Err(core::CoreError::Portfolio(_)) => {
                    *error = Some("Could not apply color.".to_owned());
                }
            }
        }
    });
    if let Some(message) = error.as_deref() {
        ui.colored_label(egui::Color32::RED, message);
    }
    let mut rgb = state.color_converter.rgb();
    let mut changed = false;
    for (index, channel) in ["R", "G", "B"].iter().enumerate() {
        if ui
            .add(egui::Slider::new(&mut rgb[index], 0..=255).text(*channel))
            .changed()
        {
            changed = true;
        }
    }
    if changed {
        state
            .dispatch(core::Command::ColorSetRgb(rgb))
            .expect("color command must succeed");
        *error = None;
    }
    let rgb = state.color_converter.rgb();
    let color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    ui.label(format!(
        "RGB: {}, {}, {}\nHex: {}",
        rgb[0],
        rgb[1],
        rgb[2],
        state.color_converter.hex_input()
    ));
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// In-memory eframe storage for testing persistence round-trips.
    struct MemoryStorage(HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn calculator_state_evaluates_and_previews_without_mutation() {
        let mut calculator = core::CalculatorState::default();
        calculator.set_input("x = 2".to_owned());
        assert!(matches!(calculator.preview(), Ok(Some(_))));
        calculator.evaluate();
        calculator.set_input("x^2".to_owned());
        calculator.evaluate();
        assert!(matches!(
            calculator.history().last().map(|entry| &entry.outcome),
            Some(core::HistoryOutcome::Value { primary, .. }) if primary == "4"
        ));
    }

    #[test]
    fn persistence_round_trips_calculator_session_through_the_adapter() {
        let mut app = PortfolioApp::default();
        app.core
            .dispatch(core::Command::CalculatorSetInput("x = 2".to_owned()))
            .expect("calculator input command must succeed");
        app.core
            .dispatch(core::Command::CalculatorEvaluate)
            .expect("calculator evaluate command must succeed");
        app.core
            .dispatch(core::Command::TextAnalyzerSetText("ab".to_owned()))
            .expect("text analyzer command must succeed");
        let mut storage = MemoryStorage(HashMap::new());
        eframe::App::save(&mut app, &mut storage);
        let raw = eframe::Storage::get_string(&storage, eframe::APP_KEY)
            .expect("stored state must exist");
        let snapshot: core::SessionSnapshot =
            ron::de::from_str(&raw).expect("state must round-trip");
        assert_eq!(snapshot.schema_version, core::CURRENT_SCHEMA_VERSION);
        assert_eq!(snapshot.calculator.session.definitions.len(), 1);
        assert_eq!(snapshot.calculator.history.len(), 1);
        assert_eq!(snapshot.text_analyzer.text, "ab");
    }

    #[test]
    fn persisted_state_does_not_include_transient_workspace() {
        let mut app = PortfolioApp::default();
        app.workspace.mobile_view = View::Calculator;
        app.workspace.calculator_open = true;
        let mut storage = MemoryStorage(HashMap::new());
        eframe::App::save(&mut app, &mut storage);
        let restored = PortfolioApp {
            core: core::CoreState::from_snapshot(load_session(&storage)),
            ..PortfolioApp::default()
        };
        assert_eq!(restored.workspace.mobile_view, View::Home);
        assert!(!restored.workspace.calculator_open);
    }

    #[test]
    fn legacy_app_record_loads_and_migrates_through_the_adapter() {
        // Record written by the pre-adapter app: the four slice snapshots
        // under their serde-renamed keys and no `schema_version`.
        let legacy = "(portfolio:(display_name:\"Ada\",headline:\"\",about:\"\",projects:[],email:\"\",website:\"\",github:\"\"),calculator:(input:\"\",history:[],session:(schema_version:1,definitions:[])),text_analyzer:(text:\"hi\"),color_converter:(hex_input:\"#336699\",rgb:(51,102,153)))";
        ron::de::from_str::<core::SessionSnapshot>(legacy)
            .expect("legacy record must parse directly");
        let mut storage = MemoryStorage(HashMap::new());
        eframe::Storage::set_string(&mut storage, eframe::APP_KEY, legacy.to_owned());
        let snapshot = load_session(&storage);
        assert_eq!(snapshot.schema_version, core::CURRENT_SCHEMA_VERSION);
        assert_eq!(snapshot.portfolio.display_name, "Ada");
        assert_eq!(snapshot.text_analyzer.text, "hi");
        assert_eq!(
            snapshot.color_converter,
            core::ColorConverterSnapshot::default()
        );
    }

    #[test]
    fn the_domain_rail_holds_exactly_six_visible_entries() {
        assert_eq!(
            PORTFOLIO_MENU_ITEMS.len() + TOOL_MENU_ITEMS.len(),
            6,
            "the rail must stay at its n ≤ 6 display cap"
        );
        // The portfolio editor is an object-level action, never a rail entry.
        assert!(
            !PORTFOLIO_MENU_ITEMS
                .iter()
                .chain(&TOOL_MENU_ITEMS)
                .any(|(_, view)| *view == View::EditPortfolio)
        );
    }

    #[test]
    fn card_grid_adapts_to_viewport_geometry_deterministically() {
        // The 700-point threshold is the same observable parameter that
        // selects the mobile layout, so both surfaces keep two columns.
        assert_eq!(card_columns(699.0), 2);
        assert_eq!(card_columns(700.0), 3);
        assert_eq!(card_columns(1280.0), 3);
        assert_eq!(card_columns(568.0), 2);
    }

    #[test]
    fn palette_lists_every_rail_workspace_and_editor_action() {
        let entries = palette_entries(false, false, false);
        for (_, view) in PORTFOLIO_MENU_ITEMS.iter().chain(&TOOL_MENU_ITEMS) {
            assert!(
                entries
                    .iter()
                    .any(|entry| entry.target == PaletteTarget::Navigate(*view))
            );
        }
        assert!(
            entries
                .iter()
                .any(|entry| entry.target == PaletteTarget::Navigate(View::EditPortfolio))
        );
        // No tile toggle in a mobile context, no Quit on the web.
        assert!(
            !entries
                .iter()
                .any(|entry| entry.target == PaletteTarget::ToggleTile)
        );
        assert!(
            !entries
                .iter()
                .any(|entry| entry.target == PaletteTarget::Quit)
        );
        let desktop = palette_entries(false, false, true);
        assert!(
            desktop
                .iter()
                .any(|entry| entry.target == PaletteTarget::ToggleTile)
        );
        let native_entries = palette_entries(false, true, true);
        assert!(
            native_entries
                .iter()
                .any(|entry| entry.target == PaletteTarget::Quit)
        );
    }

    #[test]
    fn palette_filter_is_case_insensitive_and_order_stable() {
        let entries = palette_entries(false, true, true);
        assert_eq!(palette_filter("", &entries), entries);
        // "calc" matches only the Calculator workspace.
        let calc = palette_filter("calc", &entries);
        assert_eq!(calc.len(), 1);
        assert_eq!(calc[0].target, PaletteTarget::Navigate(View::Calculator));
        // Uppercase query matches the lowercase label.
        let text = palette_filter("TEXT", &entries);
        assert_eq!(text.len(), 1);
        assert_eq!(text[0].target, PaletteTarget::Navigate(View::TextAnalyzer));
        // No match yields an empty list, not a panic.
        assert!(palette_filter("zzzz", &entries).is_empty());
    }

    #[test]
    fn tile_layout_uses_a_deterministic_non_overlapping_grid() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(404.0, 204.0));
        let panes = tile_layout(rect, 4, 4.0);
        assert_eq!(panes.len(), 4);
        // 2x2 grid, left-to-right then top-to-bottom, gap 4.
        assert_eq!(panes[0].min, egui::pos2(100.0, 50.0));
        assert_eq!(panes[1].min, egui::pos2(304.0, 50.0));
        assert_eq!(panes[2].min, egui::pos2(100.0, 154.0));
        assert_eq!(panes[3].min, egui::pos2(304.0, 154.0));
        for left in 0..panes.len() {
            for right in left + 1..panes.len() {
                assert!(!panes[left].intersects(panes[right]));
            }
        }
        assert!(panes.iter().all(|pane| rect.contains_rect(*pane)));
    }

    #[test]
    fn tile_layout_single_pane_covers_the_rect_and_empty_is_empty() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(300.0, 200.0));
        assert_eq!(tile_layout(rect, 0, 4.0), Vec::new());
        let single = tile_layout(rect, 1, 4.0);
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].min, rect.min);
        assert_eq!(single[0].max, rect.max);
        // Odd counts get a wider bottom row, never an empty pane.
        let three = tile_layout(rect, 3, 4.0);
        assert_eq!(three.len(), 3);
    }
}
