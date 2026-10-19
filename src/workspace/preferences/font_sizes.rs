//! Validated app-local font preferences, independent of rendering.

use serde::{Deserialize, Serialize};

/// Finite, bounded reference font preferences. Private fields preserve the
/// invariant needed for total equality (including after deserialization).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct FontSizes {
    ui: f32,
    code: f32,
}

impl Eq for FontSizes {}

impl Default for FontSizes {
    fn default() -> Self {
        Self {
            ui: 14.0,
            code: 12.0,
        }
    }
}

impl FontSizes {
    pub const UI_RANGE: std::ops::RangeInclusive<f32> = 11.0..=16.0;
    pub const CODE_RANGE: std::ops::RangeInclusive<f32> = 8.0..=24.0;

    pub fn ui(self) -> f32 {
        self.ui
    }
    pub fn code(self) -> f32 {
        self.code
    }

    /// Like the reference number fields, reject outliers instead of clamping;
    /// the step is one, but an explicitly entered fraction is accepted.
    pub fn with_value(mut self, code: bool, value: f32) -> Option<Self> {
        let range = if code {
            Self::CODE_RANGE
        } else {
            Self::UI_RANGE
        };
        if !value.is_finite() || !range.contains(&value) {
            return None;
        }
        if code {
            self.code = value;
        } else {
            self.ui = value;
        }
        Some(self)
    }
}

impl<'de> Deserialize<'de> for FontSizes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let mut sizes = Self::default();
        for (key, code) in [("ui", false), ("code", true)] {
            if let Some(number) = value.get(key).and_then(serde_json::Value::as_f64)
                && let Some(valid) = sizes.with_value(code, number as f32)
            {
                sizes = valid;
            }
        }
        Ok(sizes)
    }
}
