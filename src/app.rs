//! Portfolio application: state, persistence, rendering, and pure tool logic.
//!
//! The app is a client-side personal portfolio with editable browser-local
//! content, three portfolio views, and three interactive tools. On desktop
//! (content width >= 700 logical points) each destination opens as an
//! independent movable/resizable in-app window; on smaller viewports the same
//! destinations render as one full-page view at a time.

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
/// `#[serde(default)]` gives new fields defaults when deserializing old state.
#[derive(serde::Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct PortfolioApp {
    portfolio: PortfolioContent,
    calculator: CalculatorState,
    text_analyzer: TextAnalyzerState,
    color_converter: ColorConverterState,

    #[serde(skip)] // transient per-session workspace state
    workspace: WorkspaceState,
}

/// Destinations available from the menus. Home is the always-visible launcher
/// and is not a desktop window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Home,
    About,
    Projects,
    Contact,
    EditPortfolio,
    Calculator,
    TextAnalyzer,
    ColorConverter,
}

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

/// The accent color for the current (resolved) theme.
fn accent_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.style().visuals.dark_mode {
        ACCENT_DARK
    } else {
        ACCENT_LIGHT
    }
}

/// Apply the app-wide visual style to both dark and light themes: a teal
/// accent, rounded corners, and a consistent spacing scale. Runs each frame so
/// the System/Dark/Light theme buttons keep working.
fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        let accent = if style.visuals.dark_mode {
            ACCENT_DARK
        } else {
            ACCENT_LIGHT
        };

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

/// Authorable portfolio content. All fields default to empty; visitor views
/// show neutral messages until content is authored in the editor.
#[derive(serde::Deserialize, serde::Serialize, Default)]
#[serde(default)]
struct PortfolioContent {
    display_name: String,
    headline: String,
    about: String,
    projects: Vec<Project>,
    email: String,
    website: String,
    github: String,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
#[serde(default)]
struct Project {
    title: String,
    summary: String,
    url: String,
}

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
    color_error: Option<&'static str>,
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

#[derive(serde::Serialize)]
struct CalculatorState {
    input: String,
    history: Vec<HistoryEntry>,
    session: calculator_engine::SessionSnapshot,
    #[serde(skip)]
    runtime: CalculatorRuntime,
}

impl Default for CalculatorState {
    fn default() -> Self {
        Self {
            input: String::new(),
            history: Vec::new(),
            session: calculator_engine::SessionSnapshot {
                schema_version: 1,
                definitions: Vec::new(),
            },
            runtime: CalculatorRuntime::default(),
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(default)]
struct CalculatorStateWire {
    input: String,
    history: Vec<HistoryEntry>,
    session: calculator_engine::SessionSnapshot,
    left: Option<String>,
    right: Option<String>,
    operation: Option<LegacyOperation>,
    result: Option<String>,
}

impl Default for CalculatorStateWire {
    fn default() -> Self {
        Self {
            input: String::new(),
            history: Vec::new(),
            session: default_session(),
            left: None,
            right: None,
            operation: None,
            result: None,
        }
    }
}

fn default_session() -> calculator_engine::SessionSnapshot {
    calculator_engine::SessionSnapshot {
        schema_version: 1,
        definitions: Vec::new(),
    }
}

#[derive(serde::Deserialize)]
enum LegacyOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl LegacyOperation {
    fn symbol(&self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
        }
    }
}

impl<'de> serde::Deserialize<'de> for CalculatorState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = CalculatorStateWire::deserialize(deserializer)?;
        if !wire.input.is_empty()
            || !wire.history.is_empty()
            || wire.session.schema_version != 1
            || !wire.session.definitions.is_empty()
        {
            return Ok(Self {
                input: wire.input,
                history: wire.history,
                session: wire.session,
                runtime: CalculatorRuntime::default(),
            });
        }
        let mut state = Self::default();
        if let (Some(left), Some(right)) = (wire.left, wire.right) {
            let symbol = wire.operation.as_ref().map_or("+", LegacyOperation::symbol);
            let input = format!("{left} {symbol} {right}");
            if let Some(result) = wire.result {
                state.history.push(HistoryEntry {
                    input,
                    outcome: HistoryOutcome::Value {
                        primary: result,
                        approximation: None,
                    },
                });
            } else {
                state.input = input;
            }
        }
        Ok(state)
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct HistoryEntry {
    input: String,
    outcome: HistoryOutcome,
}

#[derive(serde::Deserialize, serde::Serialize)]
enum HistoryOutcome {
    Value {
        primary: String,
        approximation: Option<String>,
    },
    Error {
        message: String,
    },
}

struct CalculatorRuntime {
    calculator: calculator_engine::Calculator,
    preview: Result<Option<calculator_engine::Evaluation>, calculator_engine::Diagnostic>,
    completions: Vec<calculator_engine::Completion>,
    completion_index: usize,
    history_cursor: Option<usize>,
    restore_warning: Option<String>,
}

impl Default for CalculatorRuntime {
    fn default() -> Self {
        Self {
            calculator: calculator_engine::Calculator::new(),
            preview: Ok(None),
            completions: Vec::new(),
            completion_index: 0,
            history_cursor: None,
            restore_warning: None,
        }
    }
}

impl CalculatorState {
    fn restore_runtime(&mut self) {
        let report = self.runtime.calculator.restore(&self.session);
        if !report.discarded.is_empty() {
            self.runtime.restore_warning = Some(
                "Stored calculator variables were reset because their format is unsupported."
                    .to_owned(),
            );
        }
    }
    fn refresh_preview(&mut self) {
        self.runtime.preview = self.runtime.calculator.preview(&self.input);
        self.runtime.completions = self
            .runtime
            .calculator
            .complete(&self.input, self.input.len());
        self.runtime.completion_index = self
            .runtime
            .completion_index
            .min(self.runtime.completions.len().saturating_sub(1));
    }
    fn evaluate(&mut self) {
        let input = self.input.trim().to_owned();
        if input.is_empty() {
            return;
        }
        let outcome = match self.runtime.calculator.evaluate(&input) {
            Ok(value) => {
                self.session = self.runtime.calculator.snapshot();
                self.runtime.restore_warning = None;
                HistoryOutcome::Value {
                    primary: value.primary,
                    approximation: value.approximation,
                }
            }
            Err(error) => HistoryOutcome::Error {
                message: error.message,
            },
        };
        self.history.push(HistoryEntry { input, outcome });
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        self.runtime.history_cursor = None;
        self.input.clear();
        self.refresh_preview();
    }
}

/// Text analyzer input state. Persisted so the last session's text survives.
#[derive(serde::Deserialize, serde::Serialize, Default)]
#[serde(default)]
struct TextAnalyzerState {
    text: String,
}

/// Derived, live-updating statistics; never serialized.
struct TextStats {
    characters: usize,
    words: usize,
    lines: usize,
}

fn analyze_text(text: &str) -> TextStats {
    let characters = text.chars().count();
    let words = text.split_whitespace().count();
    let lines = if text.is_empty() {
        0
    } else {
        text.split('\n').count()
    };
    TextStats {
        characters,
        words,
        lines,
    }
}

/// Color converter state. Persisted input and RGB values survive a reload.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct ColorConverterState {
    hex_input: String,
    rgb: [u8; 3],
}

impl Default for ColorConverterState {
    fn default() -> Self {
        Self {
            hex_input: "#336699".to_owned(),
            rgb: [51, 102, 153],
        }
    }
}

/// Parse a hex color: six ASCII hexadecimal digits with an optional leading `#`.
fn parse_hex_color(input: &str) -> Result<[u8; 3], &'static str> {
    let hex = input
        .trim()
        .strip_prefix('#')
        .unwrap_or_else(|| input.trim());
    if hex.len() != 6 {
        return Err("Use a 6-digit hex color such as #336699.");
    }
    let mut rgb = [0u8; 3];
    for (i, part) in rgb.iter_mut().enumerate() {
        let pair = &hex[2 * i..2 * i + 2];
        *part = u8::from_str_radix(pair, 16)
            .map_err(|_err| "Use a 6-digit hex color such as #336699.")?;
    }
    Ok(rgb)
}

/// Format an RGB value as an uppercase `#RRGGBB` string.
fn format_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

impl PortfolioApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        let mut app = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Self::default()
        };
        app.calculator.restore_runtime();
        app.calculator.refresh_preview();
        app
    }

    /// True when the viewport is too narrow for simultaneous windows.
    fn is_mobile(ctx: &egui::Context) -> bool {
        ctx.content_rect().width() < 700.0
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
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        apply_style(ui.ctx());
        if Self::is_mobile(ui.ctx()) {
            self.show_mobile(ui);
            return;
        }

        egui::Panel::top("top_panel").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ui.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                ui.menu_button("Portfolio", |ui| {
                    for (label, view) in [
                        ("About", View::About),
                        ("Projects", View::Projects),
                        ("Contact", View::Contact),
                        ("Edit portfolio", View::EditPortfolio),
                    ] {
                        if ui.button(label).clicked() {
                            self.workspace.activate_view(view, false);
                            ui.close();
                        }
                    }
                });

                ui.menu_button("Tools", |ui| {
                    for (label, view) in [
                        ("Calculator", View::Calculator),
                        ("Text analyzer", View::TextAnalyzer),
                        ("Color converter", View::ColorConverter),
                    ] {
                        if ui.button(label).clicked() {
                            self.workspace.activate_view(view, false);
                            ui.close();
                        }
                    }
                });

                ui.add_space(16.0);

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        let frame = egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::same(28));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            self.show_desktop(ui);
        });
    }
}

impl PortfolioApp {
    /// Desktop layout: the Home launcher in the central panel plus one
    /// independent window per open destination.
    fn show_desktop(&mut self, ui: &mut egui::Ui) {
        // Central launcher:
        if let Some(view) = show_home(ui, &self.portfolio) {
            self.workspace.activate_view(view, false);
        }

        // All other mutable state must be borrowed disjointly from
        // `self.workspace` so `.open(&mut flag)` and the window closures can
        // both touch different fields.
        let Self {
            portfolio,
            calculator,
            text_analyzer,
            color_converter,
            workspace,
        } = self;

        window(
            &mut workspace.about_open,
            "About",
            [40.0, 90.0],
            [420.0, 300.0],
            ui.ctx(),
            |ui| show_about(ui, portfolio),
        );

        window(
            &mut workspace.projects_open,
            "Projects",
            [90.0, 130.0],
            [520.0, 420.0],
            ui.ctx(),
            |ui| show_projects(ui, portfolio),
        );

        window(
            &mut workspace.contact_open,
            "Contact",
            [140.0, 170.0],
            [420.0, 280.0],
            ui.ctx(),
            |ui| show_contact(ui, portfolio),
        );

        window(
            &mut workspace.edit_open,
            "Edit portfolio",
            [180.0, 90.0],
            [620.0, 560.0],
            ui.ctx(),
            |ui| show_portfolio_editor(ui, portfolio),
        );

        window(
            &mut workspace.calculator_open,
            "Calculator",
            [220.0, 120.0],
            [360.0, 300.0],
            ui.ctx(),
            |ui| show_calculator(ui, calculator),
        );

        window(
            &mut workspace.text_analyzer_open,
            "Text analyzer",
            [260.0, 150.0],
            [520.0, 420.0],
            ui.ctx(),
            |ui| show_text_analyzer(ui, text_analyzer),
        );

        window(
            &mut workspace.color_converter_open,
            "Color converter",
            [300.0, 180.0],
            [420.0, 380.0],
            ui.ctx(),
            |ui| show_color_converter(ui, color_converter, &mut workspace.color_error),
        );
    }

    /// Mobile layout: one full-page destination at a time.
    fn show_mobile(&mut self, ui: &mut egui::Ui) {
        // The compact top bar reuses the same menu entries plus a Home action.
        egui::Panel::top("mobile_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                if self.workspace.mobile_view != View::Home && ui.button("Home").clicked() {
                    self.workspace.mobile_view = View::Home;
                }

                ui.menu_button("Menu", |ui| {
                    for (label, view) in [
                        ("About", View::About),
                        ("Projects", View::Projects),
                        ("Contact", View::Contact),
                        ("Edit portfolio", View::EditPortfolio),
                    ] {
                        if ui.button(label).clicked() {
                            self.workspace.activate_view(view, true);
                            ui.close();
                        }
                    }
                    ui.separator();
                    for (label, view) in [
                        ("Calculator", View::Calculator),
                        ("Text analyzer", View::TextAnalyzer),
                        ("Color converter", View::ColorConverter),
                    ] {
                        if ui.button(label).clicked() {
                            self.workspace.activate_view(view, true);
                            ui.close();
                        }
                    }
                });

                ui.add_space(16.0);

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        let frame = egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::same(16));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let launch = match self.workspace.mobile_view {
                    View::Home => show_home(ui, &self.portfolio),
                    View::About => {
                        show_about(ui, &self.portfolio);
                        None
                    }
                    View::Projects => {
                        show_projects(ui, &self.portfolio);
                        None
                    }
                    View::Contact => {
                        show_contact(ui, &self.portfolio);
                        None
                    }
                    View::EditPortfolio => {
                        show_portfolio_editor(ui, &mut self.portfolio);
                        None
                    }
                    View::Calculator => {
                        show_calculator(ui, &mut self.calculator);
                        None
                    }
                    View::TextAnalyzer => {
                        show_text_analyzer(ui, &mut self.text_analyzer);
                        None
                    }
                    View::ColorConverter => {
                        show_color_converter(
                            ui,
                            &mut self.color_converter,
                            &mut self.workspace.color_error,
                        );
                        None
                    }
                };
                if let Some(view) = launch {
                    self.workspace.activate_view(view, true);
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

/// Lay the given destinations out as an equal-width card grid (3 columns on
/// desktop, 2 on mobile).
fn destination_grid(
    ui: &mut egui::Ui,
    grid_id: &'static str,
    destinations: &[Destination],
    clicked: &mut Option<View>,
) {
    let columns = if ui.available_width() >= 700.0 { 3 } else { 2 };
    let gap = ui.spacing().item_spacing.x;
    let card_w =
        ((ui.available_width() - (columns as f32 - 1.0) * gap) / columns as f32).max(150.0);
    let card_h = 76.0;

    egui::Grid::new(grid_id)
        .num_columns(columns)
        .spacing(egui::vec2(gap, gap))
        .show(ui, |ui| {
            for destination in destinations {
                if destination_card(ui, destination, egui::vec2(card_w, card_h)) {
                    *clicked = Some(destination.view);
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
        if content.email.contains('@') {
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

/// Render a URL as a hyperlink when it starts with `http://` or `https://`,
/// otherwise as plain text.
fn link_or_plain(ui: &mut egui::Ui, url: &str) {
    if url.starts_with("https://") || url.starts_with("http://") {
        ui.hyperlink_to(url, url);
    } else {
        ui.label(url);
    }
}

fn show_portfolio_editor(ui: &mut egui::Ui, content: &mut PortfolioContent) {
    ui.label("Edits are stored only in this browser.");
    ui.add_space(8.0);

    ui.label("Display name");
    ui.text_edit_singleline(&mut content.display_name);

    ui.label("Headline");
    ui.text_edit_singleline(&mut content.headline);

    ui.label("About");
    ui.text_edit_multiline(&mut content.about);

    ui.label("Email");
    ui.text_edit_singleline(&mut content.email);
    if !content.email.is_empty() && !content.email.contains('@') {
        ui.colored_label(egui::Color32::RED, "Enter an email address containing @.");
    }

    ui.label("Website");
    ui.text_edit_singleline(&mut content.website);
    if !content.website.is_empty() && !is_valid_url(&content.website) {
        ui.colored_label(
            egui::Color32::RED,
            "Links must start with http:// or https://.",
        );
    }

    ui.label("GitHub");
    ui.text_edit_singleline(&mut content.github);
    if !content.github.is_empty() && !is_valid_url(&content.github) {
        ui.colored_label(
            egui::Color32::RED,
            "Links must start with http:// or https://.",
        );
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    ui.heading("Projects");
    if ui.button("Add project").clicked() {
        content.projects.push(Project::default());
    }

    for (i, project) in content.projects.iter_mut().enumerate() {
        ui.add_space(8.0);
        ui.label(format!("Project {}", i + 1));
        ui.label("Title");
        ui.text_edit_singleline(&mut project.title);
        ui.label("Summary");
        ui.text_edit_multiline(&mut project.summary);
        ui.label("URL");
        ui.text_edit_singleline(&mut project.url);
        if !project.url.is_empty() && !is_valid_url(&project.url) {
            ui.colored_label(
                egui::Color32::RED,
                "Links must start with http:// or https://.",
            );
        }
    }
}

fn is_valid_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

#[expect(
    clippy::too_many_lines,
    reason = "calculator view keeps history, input, preview, and completion controls together"
)]
fn show_calculator(ui: &mut egui::Ui, state: &mut CalculatorState) {
    ui.heading("Scientific calculator");
    ui.label("Examples: 1/3, sqrt(2), 5 m/s to km/h, x = 2");
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            for entry in state.history.iter().rev() {
                ui.monospace(&entry.input);
                match &entry.outcome {
                    HistoryOutcome::Value {
                        primary,
                        approximation,
                    } => {
                        ui.label(primary);
                        if let Some(approximation) = approximation {
                            ui.label(approximation);
                        }
                    }
                    HistoryOutcome::Error { message } => {
                        ui.colored_label(egui::Color32::RED, message);
                    }
                }
                ui.separator();
            }
        });
    let input_id = egui::Id::new("calculator_repl_input");
    if let Some(warning) = &state.runtime.restore_warning {
        ui.colored_label(egui::Color32::YELLOW, warning);
    }
    if ui.memory(|memory| memory.has_focus(input_id)) {
        let up = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
        let down =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
        let tab = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
        if tab
            && let Some(completion) = state.runtime.completions.first().cloned()
            && state.input.get(completion.replacement.clone()).is_some()
        {
            state
                .input
                .replace_range(completion.replacement, &completion.insert);
            state.refresh_preview();
        }
        if up && !state.history.is_empty() {
            let next = state
                .runtime
                .history_cursor
                .map_or(state.history.len() - 1, |index| index.saturating_sub(1));
            state.runtime.history_cursor = Some(next);
            state.input = state.history[next].input.clone();
            state.refresh_preview();
        }
        if down && let Some(index) = state.runtime.history_cursor {
            if index + 1 < state.history.len() {
                state.runtime.history_cursor = Some(index + 1);
                state.input = state.history[index + 1].input.clone();
            } else {
                state.runtime.history_cursor = None;
                state.input.clear();
            }
            state.refresh_preview();
        }
    }
    let response = ui.add(
        egui::TextEdit::singleline(&mut state.input)
            .id(input_id)
            .hint_text("Enter an expression"),
    );
    if response.changed() {
        state.runtime.history_cursor = None;
        state.refresh_preview();
    }
    if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        state.evaluate();
        ui.memory_mut(|memory| memory.request_focus(input_id));
    }
    let completions: Vec<calculator_engine::Completion> =
        state.runtime.completions.iter().take(8).cloned().collect();
    if !completions.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for completion in completions {
                if ui.small_button(&completion.display).clicked()
                    && state.input.get(completion.replacement.clone()).is_some()
                {
                    state
                        .input
                        .replace_range(completion.replacement, &completion.insert);
                    state.refresh_preview();
                    ui.memory_mut(|memory| memory.request_focus(input_id));
                }
            }
        });
    }
    match &state.runtime.preview {
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

fn show_text_analyzer(ui: &mut egui::Ui, state: &mut TextAnalyzerState) {
    ui.add(
        egui::TextEdit::multiline(&mut state.text)
            .desired_width(f32::INFINITY)
            .desired_rows(10),
    );
    let stats = analyze_text(&state.text);
    ui.label(format!(
        "Characters: {}\nWords: {}\nLines: {}",
        stats.characters, stats.words, stats.lines
    ));
}

fn show_color_converter(
    ui: &mut egui::Ui,
    state: &mut ColorConverterState,
    error: &mut Option<&'static str>,
) {
    ui.horizontal(|ui| {
        ui.label("Hex");
        ui.add(egui::TextEdit::singleline(&mut state.hex_input).hint_text("#RRGGBB"));
        if ui.button("Apply hex").clicked() {
            match parse_hex_color(&state.hex_input) {
                Ok(rgb) => {
                    state.rgb = rgb;
                    state.hex_input = format_hex(rgb);
                    *error = None;
                }
                Err(message) => *error = Some(message),
            }
        }
    });
    if let Some(message) = *error {
        ui.colored_label(egui::Color32::RED, message);
    }
    let mut changed = false;
    for (index, channel) in ["R", "G", "B"].iter().enumerate() {
        if ui
            .add(egui::Slider::new(&mut state.rgb[index], 0..=255).text(*channel))
            .changed()
        {
            changed = true;
        }
    }
    if changed {
        state.hex_input = format_hex(state.rgb);
        *error = None;
    }
    let color = egui::Color32::from_rgb(state.rgb[0], state.rgb[1], state.rgb[2]);
    ui.label(format!(
        "RGB: {}, {}, {}\nHex: {}",
        state.rgb[0], state.rgb[1], state.rgb[2], state.hex_input
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
        let mut calculator = CalculatorState {
            input: "x = 2".to_owned(),
            ..CalculatorState::default()
        };
        calculator.refresh_preview();
        assert!(matches!(calculator.runtime.preview, Ok(Some(_))));
        calculator.evaluate();
        calculator.input = "x^2".to_owned();
        calculator.evaluate();
        assert!(
            matches!(calculator.history.last().map(|entry| &entry.outcome), Some(HistoryOutcome::Value { primary, .. }) if primary == "4")
        );
    }

    #[test]
    fn migrates_legacy_calculator_record() {
        let state: CalculatorState = ron::de::from_str(
            "(left:Some(\"2\"),right:Some(\"3\"),operation:Some(Add),result:Some(\"5\"))",
        )
        .expect("legacy record must deserialize");
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].input, "2 + 3");
        assert!(
            matches!(&state.history[0].outcome, HistoryOutcome::Value { primary, .. } if primary == "5")
        );
    }

    #[test]
    fn migrates_incomplete_legacy_calculator_to_empty_repl() {
        let state: CalculatorState = ron::de::from_str("(left:Some(\"2\"))")
            .expect("incomplete legacy record must deserialize");
        assert!(state.input.is_empty());
        assert!(state.history.is_empty());
    }

    #[test]
    fn analyze_empty_text() {
        let stats = analyze_text("");
        assert_eq!(stats.characters, 0);
        assert_eq!(stats.words, 0);
        assert_eq!(stats.lines, 0);
    }

    #[test]
    fn analyze_unicode_text() {
        let stats = analyze_text("hello 世界");
        assert_eq!(stats.characters, 8);
        assert_eq!(stats.words, 2);
        assert_eq!(stats.lines, 1);
    }

    #[test]
    fn analyze_repeated_whitespace() {
        let stats = analyze_text("  one\t two \n three ");
        assert_eq!(stats.characters, 19);
        assert_eq!(stats.words, 3);
        assert_eq!(stats.lines, 2);
    }

    #[test]
    fn analyze_trailing_newline() {
        let stats = analyze_text("hello 世界\n");
        assert_eq!(stats.characters, 9);
        assert_eq!(stats.words, 2);
        assert_eq!(stats.lines, 2);
    }

    #[test]
    fn parse_hex_with_prefix() {
        assert_eq!(
            parse_hex_color("#336699").expect("valid hex with prefix must parse"),
            [51, 102, 153]
        );
    }

    #[test]
    fn parse_hex_lowercase_no_prefix() {
        assert_eq!(
            parse_hex_color("ff0080").expect("valid lowercase hex must parse"),
            [255, 0, 128]
        );
    }

    #[test]
    fn parse_hex_wrong_length_fails() {
        assert_eq!(
            parse_hex_color("#33669").expect_err("too-short hex must be rejected"),
            "Use a 6-digit hex color such as #336699."
        );
        assert_eq!(
            parse_hex_color("#3366990").expect_err("too-long hex must be rejected"),
            "Use a 6-digit hex color such as #336699."
        );
    }

    #[test]
    fn parse_hex_non_hex_digits_fail() {
        assert_eq!(
            parse_hex_color("#33gg99").expect_err("non-hex digits must be rejected"),
            "Use a 6-digit hex color such as #336699."
        );
    }

    #[test]
    fn format_hex_canonicalizes() {
        assert_eq!(format_hex([51, 102, 153]), "#336699");
        assert_eq!(format_hex([255, 0, 128]), "#FF0080");
    }

    #[test]
    fn persistence_keeps_calculator_session_and_resets_workspace() {
        let mut app = PortfolioApp::default();
        app.calculator.input = "x = 2".to_owned();
        app.calculator.evaluate();
        app.text_analyzer.text = "ab".to_owned();
        app.workspace.mobile_view = View::Calculator;
        app.workspace.calculator_open = true;
        let mut storage = MemoryStorage(HashMap::new());
        eframe::set_value(&mut storage, eframe::APP_KEY, &app);
        let raw = eframe::Storage::get_string(&storage, eframe::APP_KEY)
            .expect("stored state must exist");
        let mut restored: PortfolioApp = ron::de::from_str(&raw).expect("state must round-trip");
        restored.calculator.restore_runtime();
        assert_eq!(restored.calculator.session.definitions.len(), 1);
        assert_eq!(restored.calculator.history.len(), 1);
        assert_eq!(restored.workspace.mobile_view, View::Home);
        assert!(!restored.workspace.calculator_open);
    }
}
