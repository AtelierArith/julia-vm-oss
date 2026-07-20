use subset_julia_vm_ir::Span;

/// The upstream Julia exception class a [`VmError`] variant surfaces as —
/// the single authority for sjulia's exception taxonomy (Issue #11146).
///
/// Before this type existed, three independent places decided what a raised
/// `VmError` "is":
///
/// 1. the raise site, which picked a variant and then wrote whatever message
///    text it liked (including a *different* class name — `VmError::TypeError`
///    carrying `"ArgumentError: ..."`, the shape behind 4 of the 5 root causes
///    Issue #10354's fixture-fallout measurement found);
/// 2. `vm_error_to_exception_value` (`vm/exec/error_handling.rs`), which
///    hard-coded a Julia struct-name string literal per arm;
/// 3. `is_catchable_vm_error`, a hand-maintained variant list that carried an
///    explicit "keep this byte-for-byte in sync" comment — i.e. a convention.
///
/// Now there is exactly one mapping ([`VmError::exception_class`], a
/// compile-time-exhaustive match with no catch-all arm): the exception object's
/// type and its catchability are both *derived* from it, so a raise site cannot
/// pick a class at all — it picks a variant, and the class follows. Adding a
/// `VmError` variant fails to compile until its class is declared here.
///
/// Every `julia_name()` below must name a struct that actually exists in
/// sjulia's Base (`subset_julia_vm/src/julia/base/error.jl`); a name that does
/// not resolve degrades silently to a raw `String` at catch time, so the
/// resolution is pinned by a test against a live VM
/// (`exception_class_julia_names_resolve_in_base_11146`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionClass {
    ArgumentError,
    AssertionError,
    BoundsError,
    DimensionMismatch,
    DivideError,
    DomainError,
    ErrorException,
    FieldError,
    InexactError,
    KeyError,
    MethodError,
    OutOfMemoryError,
    OverflowError,
    ParseError,
    StackOverflowError,
    StringIndexError,
    TypeError,
    UndefKeywordError,
    UndefRefError,
    UndefVarError,
    /// Not a Julia exception: a VM-internal / host-control error with no
    /// upstream equivalent (a host abort, or an sjulia invariant violation that
    /// catching would mask). These are the ONLY errors user Julia code cannot
    /// observe with `try`/`catch`.
    VmInternal,
}

impl ExceptionClass {
    /// The Julia exception struct this class constructs, or `None` for
    /// [`Self::VmInternal`] (no Julia exception object; uncatchable).
    pub fn julia_name(self) -> Option<&'static str> {
        Some(match self {
            Self::ArgumentError => "ArgumentError",
            Self::AssertionError => "AssertionError",
            Self::BoundsError => "BoundsError",
            Self::DimensionMismatch => "DimensionMismatch",
            Self::DivideError => "DivideError",
            Self::DomainError => "DomainError",
            Self::ErrorException => "ErrorException",
            Self::FieldError => "FieldError",
            Self::InexactError => "InexactError",
            Self::KeyError => "KeyError",
            Self::MethodError => "MethodError",
            Self::OutOfMemoryError => "OutOfMemoryError",
            Self::OverflowError => "OverflowError",
            Self::ParseError => "ParseError",
            Self::StackOverflowError => "StackOverflowError",
            Self::StringIndexError => "StringIndexError",
            Self::TypeError => "TypeError",
            Self::UndefKeywordError => "UndefKeywordError",
            Self::UndefRefError => "UndefRefError",
            Self::UndefVarError => "UndefVarError",
            Self::VmInternal => return None,
        })
    }

    /// Every Julia-exception class (i.e. all but [`Self::VmInternal`]).
    /// Used by the funnel's tests to prove each name resolves in Base.
    pub const JULIA_CLASSES: [Self; 20] = [
        Self::ArgumentError,
        Self::AssertionError,
        Self::BoundsError,
        Self::DimensionMismatch,
        Self::DivideError,
        Self::DomainError,
        Self::ErrorException,
        Self::FieldError,
        Self::InexactError,
        Self::KeyError,
        Self::MethodError,
        Self::OutOfMemoryError,
        Self::OverflowError,
        Self::ParseError,
        Self::StackOverflowError,
        Self::StringIndexError,
        Self::TypeError,
        Self::UndefKeywordError,
        Self::UndefRefError,
        Self::UndefVarError,
    ];
}

/// Runtime errors that can occur during VM execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    ErrorException(String), // error("message") - user-thrown exception
    ArgumentError(String),
    AssertionFailed(String),
    Cancelled,
    DivisionByZero,
    OutOfMemory,
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
    /// A `DimensionMismatch` whose diagnostic is a free-form message rather
    /// than a pair of dimension counts (e.g. LinearAlgebra's
    /// `Diagonal`/matrix shape checks). Before Issue #11146 these sites raised
    /// `VmError::ErrorException` with a `"DimensionMismatch: "` text prefix —
    /// the message named one class while the raised variant was another.
    DimensionMismatchMsg(String),
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
    /// A field that exists in a type's upstream layout but was never
    /// assigned a value (e.g. `Core.Binding.value` before the global is
    /// bound). Matches upstream Julia's `UndefRefError: access to undefined
    /// reference` (Issue #10067). Fieldless: upstream's `UndefRefError` has
    /// no fields either.
    UndefRefError,
    /// A `getfield`/`getproperty` access to a field name that does not exist
    /// on the type at all (as opposed to [`Self::UndefRefError`], where the
    /// field exists but is unset). Matches upstream Julia 1.12's
    /// `FieldError(type, field)` (Issue #10067).
    FieldError {
        type_name: String,
        field: String,
    },
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
    /// `UndefVarError` for a binding that is missing from a specific module
    /// scope, e.g. `SomeModule.undefined_name` (Issue #10318). Carries the
    /// scope so the message keeps the module name and renders like upstream
    /// Julia 1.12's `UndefVarError: `var` not defined in `Main.SomeModule``.
    /// `scope` is the fully-qualified scope string (e.g. `Main.SomeModule` or
    /// `Main`). Distinct from the bare `UndefVarError(String)` (which has no
    /// scope) so the 100+ bare raise sites are unaffected.
    UndefVarErrorInModule {
        var: String,
        scope: String,
    },
    UndefKeywordError(String), // Required keyword argument not provided (like Julia's UndefKeywordError)
    // Method errors
    MethodError(String), // No matching method for given argument types (like Julia's MethodError)
    // String errors
    StringIndexError {
        index: i64,
        valid_indices: (i64, i64), // (prev_valid, next_valid) or (-1, -1) if out of bounds
    },
    /// A Julia-source parse failure surfaced at runtime by `Meta.parse` /
    /// `include_string` / `eval`-of-a-string (Issue #11146). Upstream raises
    /// `Base.Meta.ParseError`; sjulia's Base defines the matching `ParseError`
    /// struct (`julia/base/error.jl`). Before the funnel these sites raised
    /// `VmError::TypeError` with a `"ParseError: "` text prefix.
    ParseError(String),
}

impl VmError {
    /// THE exception-type funnel (Issue #11146): the one authoritative mapping
    /// from an internal `VmError` variant to the upstream Julia exception class
    /// it raises.
    ///
    /// This match is **compile-time exhaustive and has no catch-all arm** — a
    /// new `VmError` variant does not compile until its class is declared here,
    /// and it is the ONLY place a class is chosen. Both consumers derive from
    /// it rather than re-deciding:
    ///
    /// - `vm_error_to_exception_value` (`vm/exec/error_handling.rs`) takes the
    ///   exception struct's NAME from [`ExceptionClass::julia_name`], so the
    ///   object a `catch` binds cannot be of a different class than the variant
    ///   raised (it used to hard-code a name literal per arm);
    /// - [`Self::is_catchable`] is `julia_name().is_some()`, replacing the
    ///   hand-synced variant list that carried a "keep byte-for-byte in sync"
    ///   comment.
    ///
    /// What this cannot make impossible on its own is a raise site writing a
    /// *message* that names a different class (`VmError::TypeError` whose text
    /// begins `"ArgumentError: "`). That residue is a free-form `String`, so it
    /// is enforced instead by `scripts/check_exception_taxonomy_funnel.sh`,
    /// which fails any construction whose message literal opens with a Julia
    /// exception class name that contradicts the variant's class here.
    pub fn exception_class(&self) -> ExceptionClass {
        match self {
            Self::ErrorException(_) => ExceptionClass::ErrorException,
            Self::ArgumentError(_) => ExceptionClass::ArgumentError,
            Self::AssertionFailed(_) => ExceptionClass::AssertionError,
            Self::DivisionByZero => ExceptionClass::DivideError,
            Self::OutOfMemory => ExceptionClass::OutOfMemoryError,
            Self::StackOverflow => ExceptionClass::StackOverflowError,
            // Array / range / tuple / field index errors are all upstream
            // `BoundsError`s (`julia/base/boot.jl`).
            Self::IndexOutOfBounds { .. }
            | Self::RangeIndexOutOfBounds { .. }
            | Self::TupleIndexOutOfBounds { .. }
            | Self::FieldIndexOutOfBounds { .. }
            | Self::TupleDestructuringMismatch { .. } => ExceptionClass::BoundsError,
            Self::DimensionMismatch { .. }
            | Self::MatMulDimensionMismatch { .. }
            | Self::BroadcastDimensionMismatch { .. }
            | Self::DimensionMismatchMsg(_) => ExceptionClass::DimensionMismatch,
            // Upstream raises `ArgumentError("array must be non-empty")` for
            // `pop!` on an empty collection (`julia/base/array.jl`).
            Self::EmptyArrayPop | Self::EmptyRange | Self::EmptyTuple => {
                ExceptionClass::ArgumentError
            }
            Self::TypeError(_) => ExceptionClass::TypeError,
            Self::UndefRefError => ExceptionClass::UndefRefError,
            // Julia 1.12's `FieldError(type, field)`: the field does not exist
            // on the type at all (Issue #10067).
            Self::FieldError { .. } | Self::NamedTupleFieldNotFound(_) => {
                ExceptionClass::FieldError
            }
            Self::InexactError(_) => ExceptionClass::InexactError,
            Self::DomainError(_) => ExceptionClass::DomainError,
            Self::OverflowError(_) => ExceptionClass::OverflowError,
            // Upstream: `setfield!: immutable struct of type T cannot be
            // changed` is a plain `ErrorException` (Issue #10511).
            Self::ImmutableFieldAssign(_) | Self::NamedTupleLengthMismatch { .. } => {
                ExceptionClass::ErrorException
            }
            Self::DictKeyNotFound(_) | Self::InvalidDictKey(_) => ExceptionClass::KeyError,
            Self::UndefVarError(_) | Self::UndefVarErrorInModule { .. } => {
                ExceptionClass::UndefVarError
            }
            Self::UndefKeywordError(_) => ExceptionClass::UndefKeywordError,
            Self::MethodError(_) => ExceptionClass::MethodError,
            Self::StringIndexError { .. } => ExceptionClass::StringIndexError,
            Self::ParseError(_) => ExceptionClass::ParseError,

            // `NotImplemented` is an sjulia-only "feature gap" sentinel with no
            // upstream equivalent — but it IS user-reachable (any construct
            // sjulia has not implemented), so it must still be observable as a
            // real Julia exception. Issue #8664 mapped it to `None`, which made
            // a `catch` bind a raw `String` (`typeof(e) == String`, not even an
            // `Exception` subtype — the defect Issue #11146 names explicitly).
            // The funnel's invariant is "catchable <=> has an exception object",
            // so it maps to `ErrorException`, Julia's generic catchable failure.
            Self::NotImplemented(_) => ExceptionClass::ErrorException,

            // ── VM-internal: no Julia exception, deliberately uncatchable ────
            // `Cancelled` is a host abort (iOS/WASM "stop" button); catching it
            // would let user code veto the host. The rest indicate sjulia
            // implementation bugs (mismatched push/pop, an invariant violation,
            // an unreachable opcode) that catching would mask.
            Self::Cancelled
            | Self::StackUnderflow
            | Self::InternalError(_)
            | Self::UnknownBroadcastOp(_)
            | Self::InvalidInstruction => ExceptionClass::VmInternal,
        }
    }

    /// Whether user Julia code can observe this error with `try`/`catch`
    /// (Issue #10406, re-derived through the funnel by Issue #11146).
    ///
    /// Exactly `exception_class().julia_name().is_some()`: catchable if and
    /// only if the funnel can build a Julia exception object for it. The two
    /// properties can no longer drift apart, which the previous hand-synced
    /// list (and its "INVARIANT: keep byte-for-byte in sync" comment) could.
    pub fn is_catchable(&self) -> bool {
        self.exception_class().julia_name().is_some()
    }

    /// Create a TypeError for "{instruction}: expected {expected}, got {value}" patterns (Issue #2927).
    pub fn type_error_expected(
        instruction: &str,
        expected: &str,
        got: &impl std::fmt::Debug,
    ) -> Self {
        Self::TypeError(format!(
            "{}: expected {}, got {:?}",
            instruction, expected, got
        ))
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
        Self::MethodError(format!("unsupported {} operation: {:?}", type_combo, op))
    }
}

/// Strip the internal synthetic loop-local suffix that scope-resolution appends
/// when it re-binds a loop-captured global as a fresh local:
/// `##softlocal<N>` for file-mode soft scope (Issue #9210) and `##letlocal<N>`
/// for hard-scope `let` localization (Issue #9284). User-facing messages must
/// show the original name (`total`), matching upstream's
/// `UndefVarError: \`total\` not defined`. Names without either marker are
/// returned unchanged.
fn strip_softlocal_suffix(name: &str) -> &str {
    for marker in ["##softlocal", "##letlocal"] {
        // Only strip when everything after the marker is the numeric counter, so
        // an unrelated `#`-mangled name is never truncated.
        if let Some((base, suffix)) = name.split_once(marker) {
            if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
                return base;
            }
        }
    }
    name
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ErrorException(msg) => write!(f, "ErrorException: {}", msg),
            Self::ArgumentError(msg) => write!(f, "ArgumentError: {}", msg),
            Self::AssertionFailed(msg) => write!(f, "AssertionError: {}", msg),
            Self::Cancelled => write!(f, "Execution cancelled"),
            Self::DivisionByZero => write!(f, "Division by zero"),
            Self::OutOfMemory => write!(f, "OutOfMemoryError()"),
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
            Self::DimensionMismatchMsg(msg) => write!(f, "DimensionMismatch: {}", msg),
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
            Self::UndefRefError => write!(f, "UndefRefError: access to undefined reference"),
            Self::FieldError { type_name, field } => {
                write!(f, "FieldError: type {} has no field `{}`", type_name, field)
            }
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
            Self::UndefVarError(name) => {
                let stripped = strip_softlocal_suffix(name);
                if stripped.len() != name.len() {
                    // The name carried a synthetic `##softlocal`/`##letlocal`
                    // rename marker, so this is a read-before-write of a
                    // scope-localized variable (file-mode soft scope, Issue #9210,
                    // or a hard-scope `let`, Issue #9284). Match upstream
                    // `julia file.jl`'s phrasing exactly — the "in local scope"
                    // suffix plus the shadowing suggestion (Issue #9283).
                    write!(
                        f,
                        "UndefVarError: `{stripped}` not defined in local scope\n\
                         Suggestion: check for an assignment to a local variable that shadows a global of the same name."
                    )
                } else {
                    write!(f, "UndefVarError: `{name}` not defined")
                }
            }
            // Issue #10318: module-scoped undef keeps the scope in the message,
            // matching upstream Julia 1.12's `not defined in `<scope>`` phrasing.
            Self::UndefVarErrorInModule { var, scope } => {
                write!(f, "UndefVarError: `{var}` not defined in `{scope}`")
            }
            Self::UndefKeywordError(name) => write!(
                f,
                "UndefKeywordError: keyword argument `{}` not assigned",
                name
            ),
            // Method errors
            Self::MethodError(msg) => write!(f, "MethodError: {}", msg),
            // Parse errors (Meta.parse / include_string / eval of a string)
            Self::ParseError(msg) => write!(f, "ParseError: {}", msg),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmStackFrame {
    pub function: String,
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

    /// One representative value per `VmError` variant. The `match` in
    /// [`VmError::exception_class`] is exhaustive, so this list is the
    /// test-side mirror; `every_vm_error_variant_has_a_sample_11146` keeps it
    /// honest by requiring one sample per variant name declared in this file.
    fn sample_errors() -> Vec<VmError> {
        vec![
            VmError::ErrorException("e".into()),
            VmError::ArgumentError("e".into()),
            VmError::AssertionFailed("e".into()),
            VmError::Cancelled,
            VmError::DivisionByZero,
            VmError::OutOfMemory,
            VmError::StackOverflow,
            VmError::StackUnderflow,
            VmError::InvalidInstruction,
            VmError::IndexOutOfBounds {
                indices: vec![1],
                shape: vec![0],
            },
            VmError::DimensionMismatch {
                expected: 1,
                got: 2,
            },
            VmError::DimensionMismatchMsg("e".into()),
            VmError::MatMulDimensionMismatch {
                a_shape: vec![1],
                b_shape: vec![2],
            },
            VmError::BroadcastDimensionMismatch {
                a_shape: vec![1],
                b_shape: vec![2],
            },
            VmError::EmptyArrayPop,
            VmError::RangeIndexOutOfBounds {
                index: 1,
                length: 0,
            },
            VmError::EmptyRange,
            VmError::TypeError("e".into()),
            VmError::UndefRefError,
            VmError::FieldError {
                type_name: "T".into(),
                field: "f".into(),
            },
            VmError::InexactError("Int64(1.5)".into()),
            VmError::DomainError("e".into()),
            VmError::OverflowError("e".into()),
            VmError::UnknownBroadcastOp("e".into()),
            VmError::FieldIndexOutOfBounds {
                index: 1,
                field_count: 0,
            },
            VmError::ImmutableFieldAssign("T".into()),
            VmError::NotImplemented("e".into()),
            VmError::InternalError("e".into()),
            VmError::TupleIndexOutOfBounds {
                index: 1,
                length: 0,
            },
            VmError::EmptyTuple,
            VmError::TupleDestructuringMismatch {
                expected: 1,
                got: 2,
            },
            VmError::NamedTupleFieldNotFound("f".into()),
            VmError::NamedTupleLengthMismatch {
                names_count: 1,
                values_count: 2,
            },
            VmError::DictKeyNotFound("k".into()),
            VmError::InvalidDictKey("k".into()),
            VmError::UndefVarError("x".into()),
            VmError::UndefVarErrorInModule {
                var: "x".into(),
                scope: "Main".into(),
            },
            VmError::UndefKeywordError("k".into()),
            VmError::MethodError("e".into()),
            VmError::StringIndexError {
                index: 2,
                valid_indices: (1, 3),
            },
            VmError::ParseError("e".into()),
        ]
    }

    /// The funnel's test-side sample list must cover every declared variant, or
    /// the taxonomy tests below silently stop testing a variant (Issue #11146).
    #[test]
    fn every_vm_error_variant_has_a_sample_11146() {
        let source = include_str!("error.rs");
        let enum_body = source
            .split("pub enum VmError {")
            .nth(1)
            .and_then(|s| s.split("\n}\n").next())
            .expect("VmError enum body");
        let declared: Vec<String> = enum_body
            .lines()
            // A variant line may carry a trailing `//` comment
            // (`ErrorException(String), // error("message")`), so strip the
            // comment BEFORE testing the line's shape — otherwise those
            // variants are silently skipped and this test stops guarding them,
            // which is the same "passes vacuously" failure it exists to prevent.
            .map(|line| line.split("//").next().unwrap_or_default().trim())
            .filter(|line| {
                line.chars().next().is_some_and(char::is_uppercase)
                    && (line.ends_with('{') || line.ends_with(','))
            })
            .map(|line| {
                line.trim_end_matches(&['{', ',', ' '][..])
                    .split(['(', ' '])
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .filter(|name| !name.is_empty())
            .collect();
        let samples = sample_errors();
        let sampled: Vec<String> = samples
            .iter()
            .map(|e| {
                format!("{:?}", e)
                    .split(['(', ' '])
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        for name in &declared {
            assert!(
                sampled.contains(name),
                "VmError::{name} has no sample in sample_errors(); the Issue #11146 taxonomy \
                 tests would silently skip it"
            );
        }
        assert_eq!(
            declared.len(),
            samples.len(),
            "sample_errors() must carry exactly one sample per VmError variant"
        );
    }

    /// Catchability is derived from the funnel, not from a parallel list
    /// (Issue #11146; supersedes the hand-synced #10406 invariant comment).
    #[test]
    fn is_catchable_is_derived_from_the_funnel_11146() {
        for err in sample_errors() {
            assert_eq!(
                err.is_catchable(),
                err.exception_class().julia_name().is_some(),
                "{err:?}: catchability must be exactly 'the funnel can build an exception object'"
            );
        }
    }

    /// The six VM-internal errors are the complete uncatchable set: everything
    /// else a user can trigger must surface as a real Julia exception class, so
    /// a `catch` can never bind a value that is not an `Exception` subtype.
    #[test]
    fn only_vm_internal_errors_are_uncatchable_11146() {
        let uncatchable: Vec<String> = sample_errors()
            .iter()
            .filter(|e| !e.is_catchable())
            .map(|e| {
                format!("{:?}", e)
                    .split(['(', ' '])
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        assert_eq!(
            uncatchable,
            vec![
                "Cancelled",
                "StackUnderflow",
                "InvalidInstruction",
                "UnknownBroadcastOp",
                "InternalError",
            ],
            "the uncatchable set changed; every other VmError must map to a Julia exception class"
        );
    }

    /// A message must never name a class that contradicts its variant — the
    /// Issue #10354/#11163 shape (`TypeError` carrying `"ArgumentError: ..."`).
    /// Enforced repo-wide by `scripts/check_exception_taxonomy_funnel.sh`; this
    /// pins the property for the messages this crate itself formats.
    #[test]
    fn display_never_contradicts_the_variant_class_11146() {
        for err in sample_errors() {
            let rendered = err.to_string();
            let own = err.exception_class().julia_name();
            for class in ExceptionClass::JULIA_CLASSES {
                let name = class.julia_name().unwrap_or_default();
                if Some(name) == own {
                    continue;
                }
                assert!(
                    !rendered.starts_with(&format!("{name}: ")),
                    "{err:?} renders as {rendered:?}, which opens with the class name of a \
                     DIFFERENT exception ({name}) than the funnel assigns it ({own:?})"
                );
            }
        }
    }

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
