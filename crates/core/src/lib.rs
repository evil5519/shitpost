//! Domain core for the portfolio application.

#![warn(clippy::all, rust_2018_idioms)]

pub mod calculator;
pub mod color_converter;
pub mod portfolio;
pub mod session;
pub mod text_analyzer;

pub use calculator::{CalculatorState, HistoryEntry, HistoryOutcome};
pub use color_converter::{ColorConverterState, ColorError, format_hex, parse_hex_color};
pub use portfolio::{
    Portfolio, PortfolioError, PortfolioField, Project, ProjectField, View, is_valid_email,
    is_valid_url,
};
pub use session::{
    CURRENT_SCHEMA_VERSION, CalculatorSnapshot, ColorConverterSnapshot, PortfolioSnapshot,
    SessionSnapshot, TextAnalyzerSnapshot, migrate,
};
pub use text_analyzer::{TextAnalyzerState, TextStats, analyze_text};

/// Central command stream consumed by the domain core.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    Navigate(View),
    AddProject,
    SetPortfolioField {
        field: PortfolioField,
        value: String,
    },
    SetProjectField {
        index: usize,
        field: ProjectField,
        value: String,
    },
    CalculatorSetInput(String),
    CalculatorComplete {
        replacement: std::ops::Range<usize>,
        insert: String,
    },
    CalculatorHistoryUp,
    CalculatorHistoryDown,
    CalculatorEvaluate,
    CalculatorClearHistory,
    TextAnalyzerSetText(String),
    ColorApplyHex(String),
    ColorSetRgb([u8; 3]),
}

/// Domain command failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoreError {
    Portfolio(PortfolioError),
    Color(ColorError),
}

/// Domain state and central command dispatcher.
pub struct CoreState {
    pub portfolio: Portfolio,
    pub calculator: CalculatorState,
    pub text_analyzer: TextAnalyzerState,
    pub color_converter: ColorConverterState,
    pub active_view: View,
    persisted: SessionSnapshot,
}

impl Default for CoreState {
    fn default() -> Self {
        Self::from_snapshot(SessionSnapshot {
            schema_version: CURRENT_SCHEMA_VERSION,
            ..SessionSnapshot::default()
        })
    }
}

impl CoreState {
    /// Restores the domain-owned fields from a persisted session.
    #[must_use]
    pub fn from_snapshot(snapshot: SessionSnapshot) -> Self {
        Self {
            portfolio: Portfolio::from_snapshot(snapshot.portfolio.clone()),
            calculator: CalculatorState::from_snapshot(snapshot.calculator.clone()),
            text_analyzer: TextAnalyzerState::from_snapshot(snapshot.text_analyzer.clone()),
            color_converter: ColorConverterState::from_snapshot(snapshot.color_converter.clone()),
            active_view: View::Home,
            persisted: snapshot,
        }
    }

    /// Applies one command to the domain state.
    ///
    /// # Errors
    /// Returns a structured domain error when a command cannot be applied.
    pub fn dispatch(&mut self, command: Command) -> Result<(), CoreError> {
        match command {
            Command::Navigate(view) => self.active_view = view,
            Command::AddProject => self.portfolio.add_project(),
            Command::SetPortfolioField { field, value } => self.portfolio.set_field(field, value),
            Command::SetProjectField {
                index,
                field,
                value,
            } => self
                .portfolio
                .set_project_field(index, field, value)
                .map_err(CoreError::Portfolio)?,
            Command::CalculatorSetInput(input) => self.calculator.set_input(input),
            Command::CalculatorComplete {
                replacement,
                insert,
            } => {
                self.calculator.complete(replacement, &insert);
            }
            Command::CalculatorHistoryUp => self.calculator.history_up(),
            Command::CalculatorHistoryDown => self.calculator.history_down(),
            Command::CalculatorEvaluate => self.calculator.evaluate(),
            Command::CalculatorClearHistory => self.calculator.clear_history(),
            Command::TextAnalyzerSetText(text) => self.text_analyzer.set_text(text),
            Command::ColorApplyHex(input) => self
                .color_converter
                .apply_hex(&input)
                .map_err(CoreError::Color)?,
            Command::ColorSetRgb(rgb) => self.color_converter.set_rgb(rgb),
        }
        Ok(())
    }

    /// Creates a framework-independent persisted snapshot without discarding
    /// slices that have not yet migrated into the core.
    #[must_use]
    pub fn snapshot(&self) -> SessionSnapshot {
        let mut snapshot = self.persisted.clone();
        snapshot.schema_version = CURRENT_SCHEMA_VERSION;
        snapshot.portfolio = self.portfolio.snapshot();
        snapshot.calculator = self.calculator.snapshot();
        snapshot.text_analyzer = self.text_analyzer.snapshot();
        snapshot.color_converter = self.color_converter.snapshot();
        snapshot
    }
}
