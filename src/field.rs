//! Structured field values captured from `tracing` spans and events.
//!
//! This module owns the crate's field representation. Capture layers convert
//! `tracing` visitor callbacks into [`FieldValue`] values, and display code later
//! formats those values without needing access to the original subscriber context.

use std::error::Error;
use std::fmt;

use indexmap::IndexMap;
use tracing::field::{Field, Visit};
use tracing_subscriber::field::VisitOutput;

/// Ordered map of structured tracing fields.
///
/// Field order follows the order in which `tracing` records the fields. The display
/// layer keeps that order so output remains close to `tracing_subscriber::fmt`.
pub type FieldMap = IndexMap<String, FieldValue>;

/// A structured field value captured from a tracing span or event.
///
/// The original values supplied to `tracing` may borrow local data, so the store owns
/// all captured values. Values that do not have a more precise visitor callback are
/// stored as [`FieldValue::Debug`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FieldValue {
    /// A signed integer field.
    I64(i64),

    /// An unsigned integer field.
    U64(u64),

    /// A floating point field.
    F64(f64),

    /// A boolean field.
    Bool(bool),

    /// A string field.
    Str(String),

    /// An error field formatted through its display representation.
    Error(String),

    /// A field captured through its [`fmt::Debug`] representation.
    Debug(String),
}

impl fmt::Display for FieldValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I64(value) => write!(f, "{value}"),
            Self::U64(value) => write!(f, "{value}"),
            Self::F64(value) => write!(f, "{value}"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Str(value) => f.write_str(value),
            Self::Error(value) => f.write_str(value),
            Self::Debug(value) => f.write_str(value),
        }
    }
}

impl FieldValue {
    pub(crate) fn matches_text(&self, needle: &str) -> bool {
        match self {
            Self::I64(value) => value.to_string().contains(needle),
            Self::U64(value) => value.to_string().contains(needle),
            Self::F64(value) => value.to_string().contains(needle),
            Self::Bool(value) => value.to_string().contains(needle),
            Self::Str(value) | Self::Error(value) | Self::Debug(value) => value.contains(needle),
        }
    }
}

/// Visitor that records tracing fields into an owned [`FieldMap`].
#[derive(Debug, Default)]
pub(crate) struct FieldVisitor {
    fields: FieldMap,
}

impl FieldVisitor {
    pub(crate) fn record_with(mut self, record: impl FnOnce(&mut Self)) -> FieldMap {
        record(&mut self);
        self.fields
    }

    fn insert(&mut self, field: &Field, value: FieldValue) {
        self.fields.insert(field.name().to_owned(), value);
    }
}

impl Visit for FieldVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, FieldValue::F64(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, FieldValue::I64(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, FieldValue::U64(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, FieldValue::Bool(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, FieldValue::Str(value.to_owned()));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn Error + 'static)) {
        self.insert(field, FieldValue::Error(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert(field, FieldValue::Debug(format!("{value:?}")));
    }
}

impl VisitOutput<FieldMap> for FieldVisitor {
    fn finish(self) -> FieldMap {
        self.fields
    }
}
