//! Deterministic text analyzer domain state.

use crate::session::TextAnalyzerSnapshot;
use serde::{Deserialize, Serialize};

/// Persisted text analyzer input.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct TextAnalyzerState {
    text: String,
}

/// Derived text statistics; never persisted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextStats {
    pub characters: usize,
    pub words: usize,
    pub lines: usize,
}

impl TextAnalyzerState {
    #[must_use]
    pub fn from_snapshot(snapshot: TextAnalyzerSnapshot) -> Self {
        Self {
            text: snapshot.text,
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> TextAnalyzerSnapshot {
        TextAnalyzerSnapshot {
            text: self.text.clone(),
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }

    #[must_use]
    pub fn stats(&self) -> TextStats {
        analyze_text(&self.text)
    }
}

#[must_use]
pub fn analyze_text(text: &str) -> TextStats {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_empty_text() {
        assert_eq!(
            analyze_text(""),
            TextStats {
                characters: 0,
                words: 0,
                lines: 0
            }
        );
    }

    #[test]
    fn analyze_unicode_text() {
        assert_eq!(
            analyze_text("hello 世界"),
            TextStats {
                characters: 8,
                words: 2,
                lines: 1
            }
        );
    }

    #[test]
    fn analyze_repeated_whitespace() {
        assert_eq!(
            analyze_text("  one\t two \n three "),
            TextStats {
                characters: 19,
                words: 3,
                lines: 2
            }
        );
    }

    #[test]
    fn analyze_trailing_newline() {
        assert_eq!(
            analyze_text("hello 世界\n"),
            TextStats {
                characters: 9,
                words: 2,
                lines: 2
            }
        );
    }
}
