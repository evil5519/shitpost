//! Versioned, framework-independent application persistence.

use serde::{Deserialize, Serialize};

/// Current persisted session schema.
pub const CURRENT_SCHEMA_VERSION: u16 = 1;

/// Top-level data persisted by the application.
#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct SessionSnapshot {
    pub schema_version: u16,
    pub portfolio: PortfolioSnapshot,
    pub calculator: CalculatorSnapshot,
    pub text_analyzer: TextAnalyzerSnapshot,
    pub color_converter: ColorConverterSnapshot,
}

/// Persisted portfolio content.
#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct PortfolioSnapshot {
    pub display_name: String,
    pub headline: String,
    pub about: String,
    pub projects: Vec<ProjectSnapshot>,
    pub email: String,
    pub website: String,
    pub github: String,
}

/// Persisted project content.
#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ProjectSnapshot {
    pub title: String,
    pub summary: String,
    pub url: String,
}

/// Persisted calculator data; runtime numeric values are intentionally absent.
#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct CalculatorSnapshot {
    pub input: String,
    pub history: Vec<HistorySnapshot>,
    pub session: calculator_engine::SessionSnapshot,
}

/// One persisted calculator history entry.
#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct HistorySnapshot {
    pub input: String,
    pub outcome: HistoryOutcomeSnapshot,
}

/// Persisted calculator result.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
pub enum HistoryOutcomeSnapshot {
    Value {
        primary: String,
        approximation: Option<String>,
    },
    Error {
        message: String,
    },
}

impl Default for HistoryOutcomeSnapshot {
    fn default() -> Self {
        Self::Error {
            message: String::new(),
        }
    }
}

/// Persisted text analyzer input.
#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct TextAnalyzerSnapshot {
    pub text: String,
}

/// Persisted color converter state.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ColorConverterSnapshot {
    pub hex_input: String,
    pub rgb: [u8; 3],
}

impl Default for ColorConverterSnapshot {
    fn default() -> Self {
        Self {
            hex_input: "#336699".to_owned(),
            rgb: [51, 102, 153],
        }
    }
}

/// Restores a snapshot and reports whether migration was required.
#[must_use]
pub fn migrate(mut snapshot: SessionSnapshot) -> (SessionSnapshot, bool) {
    if snapshot.schema_version == CURRENT_SCHEMA_VERSION {
        return (snapshot, false);
    }

    snapshot.schema_version = CURRENT_SCHEMA_VERSION;
    (snapshot, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_snapshot_uses_current_schema() {
        let snapshot = SessionSnapshot {
            schema_version: CURRENT_SCHEMA_VERSION,
            ..SessionSnapshot::default()
        };
        assert_eq!(snapshot.schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn migration_updates_schema_without_framework_types() {
        let snapshot = SessionSnapshot::default();
        let (migrated, changed) = migrate(snapshot);
        assert!(changed);
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    }
}
