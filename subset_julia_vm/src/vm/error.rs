use crate::span::Span;

/// Runtime errors that can occur during VM execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    ErrorException(String), // error("message") - user-thrown exception
    AssertionFailed(String),
    Cancelled,
    DivisionByZero,
    StackOverflow,
    StackUnderflow,
    InvalidInstruction,
    // Array errors
    IndexOutOfBounds {
        indices: Vec<i64>,
        shape: Vec<usize>,
    },
    DimensionMismatch {
        expected: usize,
        got: usize,
    },
    MatMulDimensionMismatch {
        a_shape: Vec<usize>,
        b_shape: Vec<usize>,
    },
    BroadcastDimensionMismatch {
        a_shape: Vec<usize>,
        b_shape: Vec<usize>,
    },
    EmptyArrayPop,
    // Range errors
    RangeIndexOutOfBounds {
        index: i64,
        length: i64,
    },
    EmptyRange,
    TypeError(String),
    InexactError(String), // Conversion to integer type with fractional part
    DomainError(String),
    OverflowError(String), // Integer overflow (e.g., factorial(21) on Int64)
    UnknownBroadcastOp(String),
    FieldIndexOutOfBounds {
        index: usize,
        field_count: usize,
    },
    ImmutableFieldAssign(String), // Attempt to modify immutable struct field
    NotImplemented(String),       // Instruction not yet implemented
    InternalError(String),        // Internal VM error (e.g., invalid function index)
    // Tuple errors
    TupleIndexOutOfBounds {
        index: i64,
        length: usize,
    },
    EmptyTuple,
    TupleDestructuringMismatch {
        expected: usize,
        got: usize,
    },
    // NamedTuple errors
    NamedTupleFieldNotFound(String),
    NamedTupleLengthMismatch {
        names_count: usize,
        values_count: usize,
    },
    // Dict errors
    DictKeyNotFound(String),
    InvalidDictKey(String),
    // Variable errors
    UndefVarError(String), // Undefined variable access (like Julia's UndefVarError)
    UndefKeywordError(String), // Required keyword argument not provided (like Julia's UndefKeywordError)
    // Method errors
    MethodError(String), // No matching method for given argument types (like Julia's MethodError)
    // String errors
    StringIndexError {
        index: i64,
        valid_indices: (i64, i64), // (prev_valid, next_valid) or (-1, -1) if out of bounds
    },
}

impl VmError {
    /// Create a TypeError for "{instruction}: expected {expected}, got {value}" patterns (Issue #2927).
    pub fn type_error_expected(instruction: &str, expected: &str, got: &impl std::fmt::Debug) -> Self {
        Self::TypeError(format!("{}: expected {}, got {:?}", instruction, expected, got))
    }

    /// Create a MethodError for "no method matching operator({type1}, {type2})" patterns (Issue #2927).
    pub fn no_method_matching_op(left_type: &str, right_type: &str) -> Self {
        Self::MethodError(format!(
            "no method matching operator({}, {})",
            left_type, right_type
        ))
    }

    /// Create a MethodError for "unsupported {type_combo} operation: {op}" patterns (Issue #2927).
    pub fn unsupported_op(type_combo: &str, op: &impl std::fmt::Debug) -> Self {
        Self::MethodError(format!(
            "unsupported {} operation: {:?}",
            type_combo, op
        ))
    }
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ErrorException(msg) => write!(f, "ErrorException: {}", msg),
            Self::AssertionFailed(msg) => write!(f, "AssertionError: {}", msg),
            Self::Cancelled => write!(f, "Execution cancelled"),
            Self::DivisionByZero => write!(f, "Division by zero"),
            Self::StackOverflow => write!(f, "Stack overflow"),
            Self::StackUnderflow => write!(f, "Stack underflow"),
            Self::InvalidInstruction => write!(f, "Invalid instruction"),
            Self::IndexOutOfBounds { indices, shape } => {
                write!(
                    f,
                    "Index {:?} out of bounds for array with shape {:?}",
                    indices, shape
                )
            }
            Self::DimensionMismatch { expected, got } => {
                write!(
                    f,
                    "Dimension mismatch: expected {} dimensions, got {}",
                    expected, got
                )
            }
            Self::MatMulDimensionMismatch { a_shape, b_shape } => {
                write!(
                    f,
                    "Matrix multiplication dimension mismatch: {:?} * {:?}",
                    a_shape, b_shape
                )
            }
            Self::BroadcastDimensionMismatch { a_shape, b_shape } => {
                write!(
                    f,
                    "Broadcast dimension mismatch: {:?} .op {:?}",
                    a_shape, b_shape
                )
            }
            Self::EmptyArrayPop => write!(f, "Cannot pop from empty array"),
            // Range errors
            Self::RangeIndexOutOfBounds { index, length } => {
                write!(
                    f,
                    "BoundsError: attempt to access {} element range at index [{}]",
                    length, index
                )
            }
            Self::EmptyRange => write!(f, "Cannot access element of empty range"),
            Self::TypeError(msg) => write!(f, "Type error: {}", msg),
            Self::InexactError(msg) => write!(f, "InexactError: {}", msg),
            Self::DomainError(msg) => write!(f, "Domain error: {}", msg),
            Self::OverflowError(msg) => write!(f, "OverflowError: {}", msg),
            Self::UnknownBroadcastOp(op) => write!(f, "Unknown broadcast operation: {}", op),
            Self::FieldIndexOutOfBounds { index, field_count } => {
                write!(
                    f,
                    "Field index {} out of bounds for struct with {} fields",
                    index, field_count
                )
            }
            Self::ImmutableFieldAssign(name) => {
                write!(f, "Cannot modify field of immutable struct: {}", name)
            }
            Self::NotImplemented(feature) => {
                write!(f, "Feature not implemented: {}", feature)
            }
            Self::InternalError(msg) => write!(f, "InternalError: {}", msg),
            // Tuple errors
            Self::TupleIndexOutOfBounds { index, length } => {
                write!(
                    f,
                    "Tuple index {} out of bounds for tuple of length {}",
                    index, length
                )
            }
            Self::EmptyTuple => write!(f, "Cannot access element of empty tuple"),
            Self::TupleDestructuringMismatch { expected, got } => {
                write!(
                    f,
                    "Tuple destructuring mismatch: expected {} elements, got {}",
                    expected, got
                )
            }
            // NamedTuple errors
            Self::NamedTupleFieldNotFound(name) => {
                write!(f, "Field '{}' not found in named tuple", name)
            }
            Self::NamedTupleLengthMismatch {
                names_count,
                values_count,
            } => {
                write!(
                    f,
                    "Named tuple length mismatch: {} names but {} values",
                    names_count, values_count
                )
            }
            // Dict errors
            Self::DictKeyNotFound(key) => write!(f, "KeyError: key {} not found", key),
            Self::InvalidDictKey(desc) => write!(f, "Invalid dictionary key: {}", desc),
            // Variable errors
            Self::UndefVarError(name) => write!(f, "UndefVarError: `{}` not defined", name),
            Self::UndefKeywordError(name) => write!(
                f,
                "UndefKeywordError: keyword argument `{}` not assigned",
                name
            ),
            // Method errors
            Self::MethodError(msg) => write!(f, "MethodError: {}", msg),
            // String errors
            Self::StringIndexError {
                index,
                valid_indices,
            } => {
                if valid_indices.0 == -1 && valid_indices.1 == -1 {
                    write!(f, "StringIndexError: invalid index [{}]", index)
                } else {
                    write!(
                        f,
                        "StringIndexError: invalid index [{}], valid nearby indices [{}], [{}]",
                        index, valid_indices.0, valid_indices.1
                    )
                }
            }
        }
    }
}

impl std::error::Error for VmError {}

/// A VmError paired with an optional source span indicating where
/// in the original Julia source the error occurred.
///
/// This wrapper is produced at the VM boundary (by [`Vm::last_error_span`])
/// and preserves the original `VmError` for pattern matching while adding
/// source location information for better debugging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedVmError {
    /// The underlying error.
    pub error: VmError,
    /// Source location where the error occurred (if available).
    pub span: Option<Span>,
}

impl SpannedVmError {
    /// Create a SpannedVmError with no span information.
    pub fn from_error(error: VmError) -> Self {
        Self { error, span: None }
    }

    /// Create a SpannedVmError with a source span.
    pub fn with_span(error: VmError, span: Span) -> Self {
        Self {
            error,
            span: Some(span),
        }
    }
}

impl From<VmError> for SpannedVmError {
    fn from(error: VmError) -> Self {
        Self::from_error(error)
    }
}

impl std::fmt::Display for SpannedVmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(span) = &self.span {
            write!(
                f,
                "{} at line {}:{}",
                self.error, span.start_line, span.start_column
            )
        } else {
            write!(f, "{}", self.error)
        }
    }
}

impl std::error::Error for SpannedVmError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spanned_error_from_error_has_no_span() {
        let err = VmError::DivisionByZero;
        let spanned = SpannedVmError::from_error(err.clone());
        assert_eq!(spanned.error, err);
        assert_eq!(spanned.span, None);
    }

    #[test]
    fn test_spanned_error_with_span() {
        let err = VmError::TypeError("bad type".to_string());
        let span = Span::new(10, 20, 3, 3, 5, 15);
        let spanned = SpannedVmError::with_span(err.clone(), span);
        assert_eq!(spanned.error, err);
        assert_eq!(spanned.span, Some(span));
    }

    #[test]
    fn test_spanned_error_from_vmerror_trait() {
        let err = VmError::StackOverflow;
        let spanned: SpannedVmError = err.clone().into();
        assert_eq!(spanned.error, err);
        assert_eq!(spanned.span, None);
    }

    #[test]
    fn test_spanned_error_display_without_span() {
        let err = VmError::DivisionByZero;
        let spanned = SpannedVmError::from_error(err);
        assert_eq!(format!("{}", spanned), "Division by zero");
    }

    #[test]
    fn test_spanned_error_display_with_span() {
        let err = VmError::TypeError("expected Int64".to_string());
        let span = Span::new(10, 20, 5, 5, 8, 18);
        let spanned = SpannedVmError::with_span(err, span);
        assert_eq!(
            format!("{}", spanned),
            "Type error: expected Int64 at line 5:8"
        );
    }

    #[test]
    fn test_spanned_error_debug_derives() {
        let err = VmError::StackUnderflow;
        let spanned = SpannedVmError::from_error(err);
        let debug_str = format!("{:?}", spanned);
        assert!(debug_str.contains("SpannedVmError"));
        assert!(debug_str.contains("StackUnderflow"));
    }

    #[test]
    fn test_spanned_error_clone_and_eq() {
        let span = Span::new(0, 5, 1, 1, 1, 6);
        let spanned = SpannedVmError::with_span(VmError::EmptyRange, span);
        let cloned = spanned.clone();
        assert_eq!(spanned, cloned);
    }

    #[test]
    fn test_spanned_error_different_spans_not_equal() {
        let err = VmError::EmptyTuple;
        let span1 = Span::new(0, 5, 1, 1, 1, 6);
        let span2 = Span::new(10, 15, 2, 2, 3, 8);
        let spanned1 = SpannedVmError::with_span(err.clone(), span1);
        let spanned2 = SpannedVmError::with_span(err, span2);
        assert_ne!(spanned1, spanned2);
    }
}
