//! Compile-time diagnostics for type inference.
//!
//! This module provides optional compile-time warnings for conservative type inference,
//! helping users understand when and why type inference falls back to `Any` (Top).
//!
//! # Overview
//!
//! During type inference, the compiler may widen types to `Any` for various reasons:
//! - Unknown function calls (no transfer function registered)
//! - Union type widening (too many elements or too complex)
//! - Recursive function cycles (detected during interprocedural analysis)
//! - Fixed-point divergence (IPO iterations don't converge)
//! - Unknown struct/field access
//!
//! This diagnostic system allows users to see these widening events and understand
//! where dynamic dispatch may occur at runtime.
//!
//! # Usage
//!
//! Diagnostics are disabled by default to avoid noisy output. Enable them via:
//! - `DiagnosticsCollector::enable()` - enable diagnostics collection
//! - `DiagnosticsCollector::disable()` - disable diagnostics collection
//! - `DiagnosticsCollector::take()` - retrieve and clear collected diagnostics

use crate::compile::lattice::widening::{MAX_UNION_COMPLEXITY, MAX_UNION_LENGTH};
use std::cell::RefCell;

/// Reason for type widening to Any (Top).
#[derive(Clone, Debug, PartialEq)]
pub enum DiagnosticReason {
    /// Unknown function: no transfer function registered.
    /// Contains the function name.
    UnknownFunction(String),

    /// Union type widened due to exceeding maximum length.
    /// Contains the number of types in the union.
    UnionTooLarge(usize),

    /// Union type widened due to exceeding maximum complexity.
    /// Contains the complexity level.
    UnionTooComplex(usize),

    /// Recursive function cycle detected during interprocedural analysis.
    /// Contains the function name(s) involved in the cycle.
    RecursiveCycle(Vec<String>),

    /// Fixed-point divergence: IPO iterations didn't converge.
    /// Contains the number of iterations before giving up.
    FixedPointDivergence(usize),

    /// Unknown struct type: struct not in the struct table.
    /// Contains the struct name if known.
    UnknownStruct(Option<String>),

    /// Unknown field access on a struct.
    /// Contains (struct_name, field_name).
    UnknownField(String, String),

    /// Array element type could not be determined.
    UnknownArrayElement,

    /// Conditional type join: control-flow sensitive types merged conservatively.
    ConditionalTypeJoin,

    /// Type conversion failed to determine target type.
    /// Contains the conversion target if known.
    ConversionUnknown(Option<String>),

    /// Generic fallback for other widening reasons.
    /// Contains a description.
    Other(String),
}

impl std::fmt::Display for DiagnosticReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosticReason::UnknownFunction(name) => {
                write!(f, "unknown function '{}'", name)
            }
            DiagnosticReason::UnionTooLarge(n) => {
                write!(
                    f,
                    "union type has {} elements (max {})",
                    n, MAX_UNION_LENGTH
                )
            }
            DiagnosticReason::UnionTooComplex(n) => {
                write!(
                    f,
                    "union type complexity {} exceeds limit (max {})",
                    n, MAX_UNION_COMPLEXITY
                )
            }
            DiagnosticReason::RecursiveCycle(names) => {
                write!(f, "recursive cycle: {}", names.join(" -> "))
            }
            DiagnosticReason::FixedPointDivergence(iters) => {
                write!(
                    f,
                    "fixed-point analysis didn't converge after {} iterations",
                    iters
                )
            }
            DiagnosticReason::UnknownStruct(name) => {
                if let Some(n) = name {
                    write!(f, "unknown struct type '{}'", n)
                } else {
                    write!(f, "unknown struct type")
                }
            }
            DiagnosticReason::UnknownField(struct_name, field) => {
                write!(f, "unknown field '{}' on struct '{}'", field, struct_name)
            }
            DiagnosticReason::UnknownArrayElement => {
                write!(f, "unknown array element type")
            }
            DiagnosticReason::ConditionalTypeJoin => {
                write!(f, "conditional types merged conservatively")
            }
            DiagnosticReason::ConversionUnknown(target) => {
                if let Some(t) = target {
                    write!(f, "type conversion to '{}' couldn't be resolved", t)
                } else {
                    write!(f, "type conversion target unknown")
                }
            }
            DiagnosticReason::Other(desc) => write!(f, "{}", desc),
        }
    }
}

/// A single type inference diagnostic (warning).
#[derive(Clone, Debug)]
pub struct TypeInferenceDiagnostic {
    /// The reason for type widening.
    pub reason: DiagnosticReason,
    /// Optional source location (line, column) if available.
    pub location: Option<(usize, usize)>,
    /// Optional variable or expression name associated with this diagnostic.
    pub context: Option<String>,
    /// The inferred type after widening (typically "Any").
    pub widened_to: String,
}

impl TypeInferenceDiagnostic {
    /// Create a new diagnostic.
    pub fn new(reason: DiagnosticReason) -> Self {
        Self {
            reason,
            location: None,
            context: None,
            widened_to: "Any".to_string(),
        }
    }

    /// Add a source location to the diagnostic.
    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        self.location = Some((line, column));
        self
    }

    /// Add context (variable/expression name) to the diagnostic.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Specify what type was widened to.
    pub fn with_widened_to(mut self, ty: impl Into<String>) -> Self {
        self.widened_to = ty.into();
        self
    }
}

impl std::fmt::Display for TypeInferenceDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "type inference warning: {}", self.reason)?;
        if let Some((line, col)) = self.location {
            write!(f, " at line {}, column {}", line, col)?;
        }
        if let Some(ctx) = &self.context {
            write!(f, " ({})", ctx)?;
        }
        write!(f, " -> {}", self.widened_to)?;
        Ok(())
    }
}

// Thread-local storage for diagnostics collector state
thread_local! {
    static DIAGNOSTICS_ENABLED: RefCell<bool> = const { RefCell::new(false) };
    static DIAGNOSTICS: RefCell<Vec<TypeInferenceDiagnostic>> = const { RefCell::new(Vec::new()) };
}

/// Collector for type inference diagnostics.
///
/// Uses thread-local storage to collect diagnostics during compilation.
/// Disabled by default to avoid overhead and noisy output.
#[derive(Debug)]
pub struct DiagnosticsCollector;

impl DiagnosticsCollector {
    /// Enable diagnostics collection.
    pub fn enable() {
        DIAGNOSTICS_ENABLED.with(|enabled| {
            *enabled.borrow_mut() = true;
        });
    }

    /// Disable diagnostics collection.
    pub fn disable() {
        DIAGNOSTICS_ENABLED.with(|enabled| {
            *enabled.borrow_mut() = false;
        });
    }

    /// Check if diagnostics collection is enabled.
    pub fn is_enabled() -> bool {
        DIAGNOSTICS_ENABLED.with(|enabled| *enabled.borrow())
    }

    /// Add a diagnostic to the collection (if enabled).
    pub fn emit(diagnostic: TypeInferenceDiagnostic) {
        if Self::is_enabled() {
            DIAGNOSTICS.with(|diags| {
                diags.borrow_mut().push(diagnostic);
            });
        }
    }

    /// Take all collected diagnostics, clearing the collection.
    pub fn take() -> Vec<TypeInferenceDiagnostic> {
        DIAGNOSTICS.with(|diags| std::mem::take(&mut *diags.borrow_mut()))
    }

    /// Clear all collected diagnostics without returning them.
    pub fn clear() {
        DIAGNOSTICS.with(|diags| {
            diags.borrow_mut().clear();
        });
    }

    /// Get the number of collected diagnostics.
    pub fn count() -> usize {
        DIAGNOSTICS.with(|diags| diags.borrow().len())
    }
}

/// Helper function to emit an unknown function diagnostic.
pub fn emit_unknown_function(function_name: &str) {
    DiagnosticsCollector::emit(
        TypeInferenceDiagnostic::new(DiagnosticReason::UnknownFunction(function_name.to_string()))
            .with_context(format!("call to {}", function_name)),
    );
}

/// Helper function to emit a union widening diagnostic.
pub fn emit_union_widened(reason: DiagnosticReason) {
    DiagnosticsCollector::emit(TypeInferenceDiagnostic::new(reason).with_widened_to("Any"));
}

/// Helper function to emit a recursive cycle diagnostic.
pub fn emit_recursive_cycle(function_names: Vec<String>) {
    DiagnosticsCollector::emit(TypeInferenceDiagnostic::new(
        DiagnosticReason::RecursiveCycle(function_names),
    ));
}

/// Helper function to emit a fixed-point divergence diagnostic.
pub fn emit_fixed_point_divergence(iterations: usize, function_names: &[String]) {
    let context = if function_names.is_empty() {
        "SCC analysis".to_string()
    } else {
        format!("functions: {}", function_names.join(", "))
    };
    DiagnosticsCollector::emit(
        TypeInferenceDiagnostic::new(DiagnosticReason::FixedPointDivergence(iterations))
            .with_context(context),
    );
}

/// Helper function to emit an unknown field diagnostic.
pub fn emit_unknown_field(struct_name: &str, field_name: &str) {
    DiagnosticsCollector::emit(TypeInferenceDiagnostic::new(
        DiagnosticReason::UnknownField(struct_name.to_string(), field_name.to_string()),
    ));
}

/// Helper function to emit a conditional type join diagnostic.
pub fn emit_conditional_join() {
    DiagnosticsCollector::emit(TypeInferenceDiagnostic::new(
        DiagnosticReason::ConditionalTypeJoin,
    ));
}

/// Helper function to emit an unknown array element type diagnostic.
pub fn emit_unknown_array_element() {
    DiagnosticsCollector::emit(TypeInferenceDiagnostic::new(
        DiagnosticReason::UnknownArrayElement,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_disabled_by_default() {
        // Clear any existing state
        DiagnosticsCollector::disable();
        DiagnosticsCollector::clear();

        assert!(!DiagnosticsCollector::is_enabled());

        // Emit should be a no-op when disabled
        emit_unknown_function("test_fn");
        assert_eq!(DiagnosticsCollector::count(), 0);
    }

    #[test]
    fn test_diagnostic_collection() {
        DiagnosticsCollector::enable();
        DiagnosticsCollector::clear();

        emit_unknown_function("foo");
        emit_unknown_function("bar");

        assert_eq!(DiagnosticsCollector::count(), 2);

        let diags = DiagnosticsCollector::take();
        assert_eq!(diags.len(), 2);
        assert_eq!(DiagnosticsCollector::count(), 0); // Should be cleared

        // Check first diagnostic
        assert!(matches!(
            &diags[0].reason,
            DiagnosticReason::UnknownFunction(name) if name == "foo"
        ));

        DiagnosticsCollector::disable();
    }

    #[test]
    fn test_diagnostic_reason_display() {
        assert_eq!(
            DiagnosticReason::UnknownFunction("foo".to_string()).to_string(),
            "unknown function 'foo'"
        );
        assert_eq!(
            DiagnosticReason::UnionTooLarge(9).to_string(),
            "union type has 9 elements (max 8)"
        );
        assert_eq!(
            DiagnosticReason::RecursiveCycle(vec!["a".to_string(), "b".to_string()]).to_string(),
            "recursive cycle: a -> b"
        );
    }

    #[test]
    fn test_diagnostic_display() {
        let diag =
            TypeInferenceDiagnostic::new(DiagnosticReason::UnknownFunction("test".to_string()))
                .with_location(10, 5)
                .with_context("call expression");

        let display = diag.to_string();
        assert!(display.contains("unknown function 'test'"));
        assert!(display.contains("line 10"));
        assert!(display.contains("call expression"));
        assert!(display.contains("Any"));
    }

    #[test]
    fn test_union_widening_diagnostics() {
        DiagnosticsCollector::enable();
        DiagnosticsCollector::clear();

        emit_union_widened(DiagnosticReason::UnionTooLarge(6));
        emit_union_widened(DiagnosticReason::UnionTooComplex(5));

        let diags = DiagnosticsCollector::take();
        assert_eq!(diags.len(), 2);

        assert!(matches!(
            &diags[0].reason,
            DiagnosticReason::UnionTooLarge(6)
        ));
        assert!(matches!(
            &diags[1].reason,
            DiagnosticReason::UnionTooComplex(5)
        ));

        DiagnosticsCollector::disable();
    }
}
