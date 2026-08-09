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
    calculator_error: Option<&'static str>,
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
            calculator_error: None,
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

/// Calculator input state. Persisted so the last session's operands,
/// operation, and result survive a reload.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct CalculatorState {
    left: String,
    right: String,
    operation: Operation,
    result: Option<String>,
}

impl Default for CalculatorState {
    fn default() -> Self {
        Self {
            left: String::new(),
            right: String::new(),
            operation: Operation::Add,
            result: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Deserialize, serde::Serialize)]
enum Operation {
    #[default]
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl Operation {
    /// Symbol shown in the operation selector.
    fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "×",
            Self::Divide => "÷",
        }
    }

    fn apply(self, left: f64, right: f64) -> f64 {
        match self {
            Self::Add => left + right,
            Self::Subtract => left - right,
            Self::Multiply => left * right,
            Self::Divide => left / right,
        }
    }
}

/// Evaluate a calculator expression. Failure messages cover invalid operands,
/// division by zero, and overflow/non-finite results.
fn calculate(left: &str, operation: Operation, right: &str) -> Result<String, &'static str> {
    let left: f64 = left
        .trim()
        .parse()
        .map_err(|_err| "Enter valid numbers in both fields.")?;
    let right: f64 = right
        .trim()
        .parse()
        .map_err(|_err| "Enter valid numbers in both fields.")?;
    let result = operation.apply(left, right);
    if matches!(operation, Operation::Divide) && right == 0.0 {
        return Err("Cannot divide by zero.");
    }
    if !result.is_finite() {
        return Err("Result is not finite.");
    }
    Ok(result.to_string())
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
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
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

        egui::CentralPanel::default().show(ui, |ui| {
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
            |ui| show_calculator(ui, calculator, &mut workspace.calculator_error),
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

        egui::CentralPanel::default().show(ui, |ui| {
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
                        show_calculator(
                            ui,
                            &mut self.calculator,
                            &mut self.workspace.calculator_error,
                        );
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

/// The Home launcher: name, headline, and launch buttons for every
/// destination. Returns the clicked destination, if any.
fn show_home(ui: &mut egui::Ui, content: &PortfolioContent) -> Option<View> {
    let name = if content.display_name.is_empty() {
        "Personal Portfolio"
    } else {
        &content.display_name
    };
    ui.heading(name);

    let headline = if content.headline.is_empty() {
        "Client-side Rust portfolio and interactive tools."
    } else {
        &content.headline
    };
    ui.label(headline);

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(12.0);

    let mut clicked = None;
    ui.horizontal(|ui| {
        if ui.button("About").clicked() {
            clicked = Some(View::About);
        }
        if ui.button("Projects").clicked() {
            clicked = Some(View::Projects);
        }
        if ui.button("Contact").clicked() {
            clicked = Some(View::Contact);
        }
    });
    ui.horizontal(|ui| {
        if ui.button("Calculator").clicked() {
            clicked = Some(View::Calculator);
        }
        if ui.button("Text analyzer").clicked() {
            clicked = Some(View::TextAnalyzer);
        }
        if ui.button("Color converter").clicked() {
            clicked = Some(View::ColorConverter);
        }
    });
    clicked
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

fn show_calculator(
    ui: &mut egui::Ui,
    state: &mut CalculatorState,
    error: &mut Option<&'static str>,
) {
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut state.left).hint_text("left"));
    });

    egui::ComboBox::from_label("Operation")
        .selected_text(state.operation.symbol())
        .show_ui(ui, |ui| {
            for operation in [
                Operation::Add,
                Operation::Subtract,
                Operation::Multiply,
                Operation::Divide,
            ] {
                ui.selectable_value(&mut state.operation, operation, operation.symbol());
            }
        });

    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut state.right).hint_text("right"));
    });

    if ui.button("Calculate").clicked() {
        match calculate(&state.left, state.operation, &state.right) {
            Ok(result) => {
                state.result = Some(result);
                *error = None;
            }
            Err(message) => {
                state.result = None;
                *error = Some(message);
            }
        }
    }

    if let Some(result) = &state.result {
        ui.add_space(8.0);
        ui.label(format!("Result: {result}"));
    }
    if let Some(message) = *error {
        ui.colored_label(egui::Color32::RED, message);
    }
}

fn show_text_analyzer(ui: &mut egui::Ui, state: &mut TextAnalyzerState) {
    ui.add(
        egui::TextEdit::multiline(&mut state.text)
            .desired_width(f32::INFINITY)
            .desired_rows(10),
    );

    let stats = analyze_text(&state.text);
    ui.add_space(8.0);
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
    for (i, channel) in ["R", "G", "B"].iter().enumerate() {
        let value = &mut state.rgb[i];
        if ui
            .add(egui::Slider::new(value, 0..=255).text(*channel))
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
    ui.add_space(8.0);
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
    fn calculate_adds() {
        assert_eq!(
            calculate("2", Operation::Add, "3").expect("2 + 3 must succeed"),
            "5"
        );
    }

    #[test]
    fn calculate_invalid_operand_fails() {
        assert_eq!(
            calculate("abc", Operation::Add, "3").expect_err("non-numeric operand must fail"),
            "Enter valid numbers in both fields."
        );
        assert_eq!(
            calculate("2", Operation::Add, "").expect_err("empty operand must fail"),
            "Enter valid numbers in both fields."
        );
    }

    #[test]
    fn calculate_divide_by_zero_fails() {
        assert_eq!(
            calculate("1", Operation::Divide, "0")
                .expect_err("division by positive zero must fail"),
            "Cannot divide by zero."
        );
        assert_eq!(
            calculate("1", Operation::Divide, "-0")
                .expect_err("division by negative zero must fail"),
            "Cannot divide by zero."
        );
    }

    #[test]
    fn calculate_non_finite_result_fails() {
        assert_eq!(
            calculate("1e308", Operation::Multiply, "1e308").expect_err("overflow must fail"),
            "Result is not finite."
        );
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
    fn persistence_keeps_data_and_resets_workspace() {
        let mut app = PortfolioApp::default();
        app.portfolio.display_name = "Ada Lovelace".to_owned();
        app.portfolio.projects.push(Project {
            title: "Analytica".to_owned(),
            summary: "The first algorithm engine".to_owned(),
            url: "https://example.com".to_owned(),
        });
        app.calculator.left = "6".to_owned();
        app.calculator.operation = Operation::Divide;
        app.calculator.right = "3".to_owned();
        app.calculator.result = Some("2".to_owned());
        app.text_analyzer.text = "ab".to_owned();
        app.color_converter.hex_input = "#ff0080".to_owned();
        app.color_converter.rgb = [255, 0, 128];
        // Simulate a modified session's transient state:
        app.workspace.mobile_view = View::Calculator;
        app.workspace.calculator_open = true;
        app.workspace.calculator_error = Some("Cannot divide by zero.");

        let mut storage = MemoryStorage(HashMap::new());
        eframe::set_value(&mut storage, eframe::APP_KEY, &app);

        let restored: PortfolioApp =
            eframe::get_value(&storage, eframe::APP_KEY).expect("state must round-trip");

        assert_eq!(restored.portfolio.display_name, "Ada Lovelace");
        assert_eq!(restored.portfolio.projects.len(), 1);
        assert_eq!(restored.portfolio.projects[0].title, "Analytica");
        assert_eq!(restored.calculator.result.as_deref(), Some("2"));
        assert_eq!(restored.calculator.operation, Operation::Divide);
        assert_eq!(restored.text_analyzer.text, "ab");
        assert_eq!(restored.color_converter.rgb, [255, 0, 128]);

        // Workspace resets even though the transient state was set before saving:
        assert_eq!(restored.workspace.mobile_view, View::Home);
        assert!(!restored.workspace.calculator_open);
        assert!(restored.workspace.calculator_error.is_none());
        assert!(restored.workspace.color_error.is_none());
    }
}
