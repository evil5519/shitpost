//! Calculator domain state, persistence, and command handling.

use crate::session::{CalculatorSnapshot, HistoryOutcomeSnapshot, HistorySnapshot};
use serde::{Deserialize, Serialize};

/// Persisted calculator state with a non-serializable live runtime.
#[derive(Default, Serialize)]
pub struct CalculatorState {
    input: String,
    history: Vec<HistoryEntry>,
    session: calculator_engine::SessionSnapshot,
    #[serde(skip)]
    runtime: CalculatorRuntime,
}

#[derive(Default, Deserialize)]
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

#[derive(Deserialize)]
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

impl<'de> Deserialize<'de> for CalculatorState {
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

#[derive(Clone, Deserialize, Serialize)]
pub struct HistoryEntry {
    pub input: String,
    pub outcome: HistoryOutcome,
}

#[derive(Clone, Deserialize, Serialize)]
pub enum HistoryOutcome {
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
    history_cursor: Option<usize>,
    restore_warning: Option<String>,
}

impl Default for CalculatorRuntime {
    fn default() -> Self {
        Self {
            calculator: calculator_engine::Calculator::new(),
            preview: Ok(None),
            completions: Vec::new(),
            history_cursor: None,
            restore_warning: None,
        }
    }
}

impl CalculatorState {
    /// Rebuilds the live calculator from the persisted session.
    pub fn restore_runtime(&mut self) {
        let report = self.runtime.calculator.restore(&self.session);
        if !report.discarded.is_empty() {
            self.runtime.restore_warning = Some(
                "Stored calculator variables were reset because their format is unsupported."
                    .to_owned(),
            );
        }
        self.refresh_preview();
    }

    /// Returns the current input for rendering.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Returns submitted history for rendering.
    #[must_use]
    pub fn history(&self) -> &[HistoryEntry] {
        &self.history
    }

    /// Returns the current preview for rendering.
    pub fn preview(
        &self,
    ) -> &Result<Option<calculator_engine::Evaluation>, calculator_engine::Diagnostic> {
        &self.runtime.preview
    }

    /// Returns available completions for rendering.
    #[must_use]
    pub fn completions(&self) -> &[calculator_engine::Completion] {
        &self.runtime.completions
    }

    /// Returns an optional restore warning for rendering.
    #[must_use]
    pub fn restore_warning(&self) -> Option<&str> {
        self.runtime.restore_warning.as_deref()
    }

    pub fn set_input(&mut self, input: String) {
        self.input = input;
        self.runtime.history_cursor = None;
        self.refresh_preview();
    }

    pub fn complete(&mut self, replacement: std::ops::Range<usize>, insert: &str) {
        if self.input.get(replacement.clone()).is_some() {
            self.input.replace_range(replacement, insert);
            self.refresh_preview();
        }
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = self
            .runtime
            .history_cursor
            .map_or(self.history.len() - 1, |index| index.saturating_sub(1));
        self.runtime.history_cursor = Some(next);
        self.input = self.history[next].input.clone();
        self.refresh_preview();
    }

    pub fn history_down(&mut self) {
        if let Some(index) = self.runtime.history_cursor {
            if index + 1 < self.history.len() {
                self.runtime.history_cursor = Some(index + 1);
                self.input = self.history[index + 1].input.clone();
            } else {
                self.runtime.history_cursor = None;
                self.input.clear();
            }
            self.refresh_preview();
        }
    }

    /// Clears the submitted history transcript. Keeps the current input and
    /// the persisted variable session so definitions survive a clear.
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.runtime.history_cursor = None;
    }

    pub fn evaluate(&mut self) {
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

    fn refresh_preview(&mut self) {
        self.runtime.preview = self.runtime.calculator.preview(&self.input);
        self.runtime.completions = self
            .runtime
            .calculator
            .complete(&self.input, self.input.len());
    }

    #[must_use]
    pub fn from_snapshot(snapshot: CalculatorSnapshot) -> Self {
        let mut state = Self {
            input: snapshot.input,
            history: snapshot
                .history
                .into_iter()
                .map(HistoryEntry::from)
                .collect(),
            session: snapshot.session,
            runtime: CalculatorRuntime::default(),
        };
        state.restore_runtime();
        state
    }

    #[must_use]
    pub fn snapshot(&self) -> CalculatorSnapshot {
        CalculatorSnapshot {
            input: self.input.clone(),
            history: self.history.iter().map(HistorySnapshot::from).collect(),
            session: self.session.clone(),
        }
    }
}

impl From<HistorySnapshot> for HistoryEntry {
    fn from(value: HistorySnapshot) -> Self {
        Self {
            input: value.input,
            outcome: value.outcome.into(),
        }
    }
}

impl From<&HistoryEntry> for HistorySnapshot {
    fn from(value: &HistoryEntry) -> Self {
        Self {
            input: value.input.clone(),
            outcome: (&value.outcome).into(),
        }
    }
}

impl From<HistoryOutcomeSnapshot> for HistoryOutcome {
    fn from(value: HistoryOutcomeSnapshot) -> Self {
        match value {
            HistoryOutcomeSnapshot::Value {
                primary,
                approximation,
            } => Self::Value {
                primary,
                approximation,
            },
            HistoryOutcomeSnapshot::Error { message } => Self::Error { message },
        }
    }
}

impl From<&HistoryOutcome> for HistoryOutcomeSnapshot {
    fn from(value: &HistoryOutcome) -> Self {
        match value {
            HistoryOutcome::Value {
                primary,
                approximation,
            } => Self::Value {
                primary: primary.clone(),
                approximation: approximation.clone(),
            },
            HistoryOutcome::Error { message } => Self::Error {
                message: message.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restoring_snapshot_rebuilds_live_calculator_runtime() {
        let mut calculator = CalculatorState::default();
        calculator.set_input("x = 2".to_owned());
        calculator.evaluate();

        let mut restored = CalculatorState::from_snapshot(calculator.snapshot());
        restored.set_input("x^2".to_owned());
        restored.evaluate();

        assert!(matches!(
            restored.history().last().map(|entry| &entry.outcome),
            Some(HistoryOutcome::Value { primary, .. }) if primary == "4"
        ));
    }

    #[test]
    fn migrates_legacy_calculator_record() {
        let state: CalculatorState = ron::de::from_str(
            "(left:Some(\"2\"),right:Some(\"3\"),operation:Some(Add),result:Some(\"5\"))",
        )
        .expect("legacy record must deserialize");
        assert_eq!(state.history().len(), 1);
        assert_eq!(state.history()[0].input, "2 + 3");
        assert!(matches!(
            &state.history()[0].outcome,
            HistoryOutcome::Value { primary, .. } if primary == "5"
        ));
    }

    #[test]
    fn migrates_incomplete_legacy_calculator_to_empty_repl() {
        let state: CalculatorState = ron::de::from_str("(left:Some(\"2\"))")
            .expect("incomplete legacy record must deserialize");
        assert!(state.input().is_empty());
        assert!(state.history().is_empty());
    }

    #[test]
    fn clear_history_drops_transcript_but_keeps_input_and_definitions() {
        let mut calculator = CalculatorState::default();
        calculator.set_input("x = 2".to_owned());
        calculator.evaluate();
        calculator.set_input("x^2".to_owned());
        calculator.evaluate();
        assert_eq!(calculator.history().len(), 2);

        calculator.set_input("x + 1".to_owned());
        calculator.clear_history();

        assert!(calculator.history().is_empty());
        assert_eq!(calculator.input(), "x + 1");
        // The variable session survives a clear, so definitions still apply.
        assert!(matches!(
            calculator.preview(),
            Ok(Some(evaluation)) if evaluation.primary == "3"
        ));
    }
}
