//! Re-export Span from crate::span for backwards compatibility.
//! The actual Span definition is in crate::span to allow use without parser feature.

pub use crate::span::Span;
