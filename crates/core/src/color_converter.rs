//! Color converter domain state, validation, and canonicalization.

use crate::session::ColorConverterSnapshot;
use serde::{Deserialize, Serialize};

/// Persisted color converter state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ColorConverterState {
    hex_input: String,
    rgb: [u8; 3],
}

impl Default for ColorConverterState {
    fn default() -> Self {
        Self::from_snapshot(ColorConverterSnapshot::default())
    }
}

/// Structured color validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorError {
    InvalidHex,
}

impl ColorError {
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidHex => "Use a 6-digit hex color such as #336699.",
        }
    }
}

impl ColorConverterState {
    #[must_use]
    pub fn from_snapshot(snapshot: ColorConverterSnapshot) -> Self {
        Self {
            hex_input: snapshot.hex_input,
            rgb: snapshot.rgb,
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> ColorConverterSnapshot {
        ColorConverterSnapshot {
            hex_input: self.hex_input.clone(),
            rgb: self.rgb,
        }
    }

    #[must_use]
    pub fn hex_input(&self) -> &str {
        &self.hex_input
    }

    #[must_use]
    pub fn rgb(&self) -> [u8; 3] {
        self.rgb
    }

    /// # Errors
    /// Returns `InvalidHex` when the input is not six hexadecimal digits.
    pub fn apply_hex(&mut self, input: &str) -> Result<(), ColorError> {
        self.hex_input = input.to_owned();
        let rgb = parse_hex_color(&self.hex_input)?;
        self.rgb = rgb;
        self.hex_input = format_hex(rgb);
        Ok(())
    }

    pub fn set_rgb(&mut self, rgb: [u8; 3]) {
        self.rgb = rgb;
        self.hex_input = format_hex(rgb);
    }
}

/// # Errors
/// Returns `InvalidHex` when the input is not six hexadecimal digits.
pub fn parse_hex_color(input: &str) -> Result<[u8; 3], ColorError> {
    let hex = input
        .trim()
        .strip_prefix('#')
        .unwrap_or_else(|| input.trim());
    let bytes = hex.as_bytes();
    if bytes.len() != 6 || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err(ColorError::InvalidHex);
    }
    let mut rgb = [0u8; 3];
    for (index, channel) in rgb.iter_mut().enumerate() {
        let pair = &bytes[index * 2..index * 2 + 2];
        *channel = u8::from_str_radix(
            std::str::from_utf8(pair).map_err(|_error| ColorError::InvalidHex)?,
            16,
        )
        .map_err(|_error| ColorError::InvalidHex)?;
    }
    Ok(rgb)
}

#[must_use]
pub fn format_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_with_prefix() {
        assert_eq!(parse_hex_color("#336699"), Ok([51, 102, 153]));
    }

    #[test]
    fn parse_hex_lowercase_no_prefix() {
        assert_eq!(parse_hex_color("ff0080"), Ok([255, 0, 128]));
    }

    #[test]
    fn parse_hex_wrong_length_fails() {
        assert_eq!(parse_hex_color("#33669"), Err(ColorError::InvalidHex));
        assert_eq!(parse_hex_color("#3366990"), Err(ColorError::InvalidHex));
    }

    #[test]
    fn parse_hex_non_hex_digits_fail() {
        assert_eq!(parse_hex_color("#33gg99"), Err(ColorError::InvalidHex));
    }

    #[test]
    fn format_hex_canonicalizes() {
        assert_eq!(format_hex([51, 102, 153]), "#336699");
        assert_eq!(format_hex([255, 0, 128]), "#FF0080");
    }
}
