use serde::{Deserialize, Serialize};

use crate::{ArrayElementType, BuiltinId, Intrinsic};
pub use crate::{
    ArrayLiteralPayload, CallDirectSlots, CallSpecializeSlots, CallVarKwargsSplat,
    DefineRuntimeNominalOperands, DynamicCallCandidate, DynamicCallOperands, GeneratorCallableSpec,
    InvokeWithKwargs, MakeGeneratorOperands, ModuleOperands, NativeIteratorKind,
    ParametricConstructorCandidate, ParametricConstructorDispatchCall, RegisterEnumOperands,
    ResolvedClosureOperands, ResolvedFunctionOperands, StaticParamBinding, StaticParametricCall,
    TypedDispatchStoreDict,
};
use half::f16;

/// Concrete scalar element type for a typed `rand(T)` / `rand(rng, T)` draw
/// (Issue #9265). Covers the bits-numeric and `Bool` types that upstream
/// `Random.rand(::Type{T})` samples uniformly over their full range. `Int` and
/// `Int64` both lower to `I64`, and `UInt`/`UInt64` to `U64`, on the 64-bit
/// host — matching the rest of the `rand` builtin lowering.
///
/// The variant order is frozen: bincode encodes it by declaration index and the
/// [`Instr`] variants that carry it are serialized into the Base cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScalarRandType {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    Bool,
    F16,
    F32,
    F64,
}

/// Comparison predicate for the fused slot-vs-constant compare-and-branch
/// instruction [`Instr::JumpIfCmpI64SlotConst`] (Issue #10105).
///
/// The predicate stored is the *jump* condition — i.e. it is already the
/// inverse of the source-level comparison, because the peephole pass folds a
/// `<cmp>I64; JumpIfZero` (jump when the comparison is FALSE) pair. For example
/// the loop guard `i < n` lowers to `LtI64; JumpIfZero(exit)` and fuses to
/// `JumpIfCmpI64SlotConst(i, n, I64Cmp::Ge, exit)` — "jump to `exit` when
/// `i >= n`".
///
/// The variant order is frozen: bincode encodes it by declaration index and it
/// is serialized into the Base cache as part of the owning [`Instr`] variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum I64Cmp {
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
}

// === Boxed operand payloads (Issue #5176) ===
//
// The `Instr` enum is stored densely in `code: Vec<Instr>` and fetched every
// dispatch cycle, so its stride is the cost of the *largest* variant. A handful
// of cold, construction-time variants carry wide `String`/`Vec` operand groups
// (>= 72 bytes) that would otherwise dictate the stride for hot single-word
// instructions such as `PushI64`. Boxing those payloads collapses each affected
// variant to a single pointer, shrinking `size_of::<Instr>()` and densifying the
// `code` array (see `test_instr_enum_size_is_compact`). These structs hold the
// original tuple fields verbatim so behaviour is unchanged.

// `strum::VariantNames` feeds `compile::precompile::enum_variant_fingerprint()`
// (Issue #8626): bincode encodes `Instr` by variant declaration index, so the
// Base cache header carries a hash of this variant-name list and stale caches
// are regenerated instead of silently misdecoding bytecode.
#[derive(Debug, Clone, Serialize, Deserialize, strum::VariantNames)]
pub enum Instr {
    // constants / locals
    PushI64(i64),
    /// Push an Int128/UInt128 literal. Boxed (Issue #5176): an inline `i128`
    /// forces 16-byte alignment on the whole `Instr` enum, rounding every
    /// variant's stride up to the next multiple of 16. Int128 literals are cold
    /// compared to `PushI64`, so boxing the payload removes that alignment tax
    /// and lets the dense `code` array pack hot instructions more tightly.
    PushI128(Box<i128>),
    PushBigInt(String),
    PushBigFloat(String), // Push a BigFloat onto the stack
    PushF64(f64),
    PushF32(f32),   // Push a Float32 onto the stack
    PushF16(f16),   // Push a Float16 onto the stack
    PushBool(bool), // Push a Bool onto the stack
    /// Push whether @boundscheck code should run in the current call frame.
    PushBoundsCheckEnabled,
    PushStr(String),
    /// Push a Julia String whose bytes are not valid UTF-8 (Issue #8995),
    /// e.g. the literal `"\xff"`. Valid-UTF-8 literals use `PushStr`.
    PushStrBytes(Vec<u8>),
    PushChar(char), // Push a Char (Unicode codepoint) onto the stack
    /// Push a malformed Julia Char by its 32-bit char pattern (Issue #8995),
    /// e.g. the literal `'\xff'`. Valid scalars use `PushChar`.
    PushCharMalformed(u32),
    PushNothing, // Push the `nothing` value onto the stack
    PushMissing, // Push the `missing` value onto the stack
    PushUndef,   // Push the `#undef` value onto the stack (for required kwargs)
    PushStdout,  // Push the `stdout` IO value onto the stack
    PushStderr,  // Push the `stderr` IO value onto the stack
    PushStdin,   // Push the `stdin` IO value onto the stack
    PushDevnull, // Push the `devnull` IO value onto the stack
    PushCNull,   // Push the `C_NULL` pointer value (Ptr{Cvoid}(0)) onto the stack
    PushEnv,     // Push the `ENV` dictionary (environment variables) onto the stack
    /// Push a Module value onto the stack with exports and publics
    /// Args: (name, exports, publics)
    ///
    /// Boxed to keep `Instr` compact (Issue #5176): this cold construction-time
    /// operand triple would otherwise inflate the enum's stride for every hot
    /// instruction in the `code` array.
    PushModule(Box<ModuleOperands>),
    PushDataType(String), // Push a DataType value (type as a value)
    PushFunction(String), // Push a function object by name
    PushResolvedFunction(Box<ResolvedFunctionOperands>), // Push a function object with resolved methods
    /// Create a closure with captured variables from the current scope.
    /// The first argument is the function name, and the second is a list of variable names to capture.
    /// At runtime, this instruction:
    /// 1. Looks up each captured variable name in the current frame
    /// 2. Creates a ClosureValue with the function name and captured values
    /// 3. Pushes the closure onto the stack
    CreateClosure {
        func_name: String,
        capture_names: Vec<String>,
    },
    /// Load a captured variable from the current closure environment.
    /// Used inside closure bodies to access captured variables.
    LoadCaptured(String),
    /// Define a function at runtime (for functions defined inside blocks).
    /// The function is registered to the dispatch table when this instruction executes.
    /// Takes the function index in the function_infos table.
    DefineFunction(usize),
    /// Activate a method introduced by runtime `@eval`.
    DefineEvalFunction(usize),
    /// Record completion of one source `using`/`import` statement.
    ///
    /// The operands identify the owning module plus the statement's stable
    /// index in that scope's `usings` vector. REPL catchable-error recovery uses
    /// the executed identities to retain exactly the reached imports without
    /// colliding with module-local indices or reviving a source-later suffix
    /// (Issue #11748).
    ActivateUsing {
        owner_module: String,
        program_index: usize,
    },
    /// Record that execution entered a source module after publishing its
    /// module binding. REPL error recovery retains exactly these reached module
    /// shells and discards source-later module bodies (Issue #11761).
    ActivateModule(String),
    /// Activate a concrete struct definition at its source position.
    ///
    /// The compiler reserves the stable `type_id` ahead of execution, while
    /// the VM keeps the corresponding metadata private until this marker runs.
    /// This mirrors `DefineEvalFunction` and prevents a later declaration from
    /// becoming visible before its source position (Issues #9784/#11546).
    DefineEvalStruct(usize),
    // Metaprogramming value instructions (for REPL persistence)
    PushSymbol(String), // Push a Symbol value (e.g., :foo)
    /// Create an Expr from args on stack. Pops `arg_count` values, creates Expr with given head.
    CreateExpr {
        head: String,
        arg_count: usize,
    },
    /// Create a QuoteNode from the top of stack
    CreateQuoteNode,
    /// Push a LineNumberNode value
    PushLineNumberNode {
        line: i64,
        file: Option<String>,
    },
    /// Push a Regex value (compiled regular expression)
    PushRegex {
        pattern: String,
        flags: String,
    },
    /// Push an Enum value for REPL persistence (@enum type values)
    PushEnum {
        type_name: String,
        value: i64,
    },
    LoadStr(String),
    StoreStr(String),
    LoadI64(String),
    StoreI64(String),
    LoadF64(String),
    StoreF64(String),
    LoadF32(String),
    StoreF32(String),
    LoadF16(String),
    StoreF16(String),
    LoadBool(String),
    StoreBool(String),
    LoadSlot(usize),
    StoreSlot(usize),
    LoadSlotI64(usize),
    StoreSlotI64(usize),
    LoadSlotF64(usize),
    StoreSlotF64(usize),
    LoadSlotBool(usize),
    StoreSlotBool(usize),
    LoadSlotF32(usize),
    StoreSlotF32(usize),
    LoadSlotF16(usize),
    StoreSlotF16(usize),
    LoadSlotStr(usize),
    StoreSlotStr(usize),
    LoadSlotChar(usize),
    StoreSlotChar(usize),
    LoadSlotNarrowInt(usize),
    StoreSlotNarrowInt(usize),
    LoadSlotNothing(usize),
    StoreSlotNothing(usize),
    LoadSlotArray(usize),
    StoreSlotArray(usize),
    LoadSlotTuple(usize),
    StoreSlotTuple(usize),
    LoadSlotNamedTuple(usize),
    StoreSlotNamedTuple(usize),
    LoadSlotDict(usize),
    StoreSlotDict(usize),
    LoadSlotSet(usize),
    StoreSlotSet(usize),
    LoadSlotStruct(usize),
    StoreSlotStruct(usize),
    LoadSlotRange(usize),
    StoreSlotRange(usize),
    LoadSlotRng(usize),
    StoreSlotRng(usize),
    LoadSlotGenerator(usize),
    StoreSlotGenerator(usize),
    LoadAny(String),  // Dynamic load - checks all type maps at runtime
    StoreAny(String), // Dynamic store - stores based on runtime type
    /// Dynamic load from the module-level (frame 0) binding, regardless of the
    /// current call frame. Emitted for reads of names declared `global x` inside
    /// a local/testset scope so slotization cannot capture a stale local slot.
    LoadGlobalAny(String),
    /// Dynamic store into the module-level (frame 0) binding, regardless of the
    /// current call frame. Emitted for assignments to names declared `global x`
    /// inside a function so the top-level binding is updated (Issues #5548,
    /// #5549). Stores by runtime type exactly like `StoreAny`.
    StoreGlobalAny(String),
    /// Load type binding from current frame's type_bindings map.
    /// Used for accessing type parameters (T) from where clauses as values.
    LoadTypeBinding(String),
    /// Load boolean Val parameter from current frame (for Val{true}/Val{false} patterns).
    /// Used when accessing type parameters from where clauses that are boolean values.
    LoadValBool(String),
    /// Load symbol Val parameter from current frame (for Val{:symbol} patterns).
    /// Used when accessing type parameters from where clauses that are symbol values.
    LoadValSymbol(String),

    // === Dynamic arithmetic ops (runtime type dispatch) ===
    // These instructions perform type-based dispatch at runtime,
    // following Julia's type promotion rules.
    // Used when parameter types are not known at compile time.
    DynamicAdd,    // Pop two values, add with type promotion, push result
    DynamicSub,    // Pop two values, subtract with type promotion, push result
    DynamicMul,    // Pop two values, multiply with type promotion, push result
    DynamicDiv,    // Pop two values, divide with type promotion, push result
    DynamicMod,    // Pop two values, modulo with type promotion, push result
    DynamicIntDiv, // Pop two values, integer division with type promotion, push result
    DynamicNeg,    // Pop one value, negate with type preservation, push result
    DynamicPow,    // Pop two values, power with type promotion, push result

    // Int64 ops
    AddI64,
    SubI64,
    MulI64,
    ModI64,
    IncI64,
    DupI64, // duplicate top I64 on stack
    Dup,    // duplicate top of stack (any type)
    NegI64,

    // === Fused instructions for performance ===
    // These combine common instruction patterns to reduce overhead

    // Load-Op fusion: Load variable + operate with stack top
    LoadAddI64(String), // Load var, pop stack, add, push result
    LoadSubI64(String), // Load var, pop stack, subtract (var - stack), push
    LoadMulI64(String), // Load var, pop stack, multiply, push
    LoadModI64(String), // Load var, pop stack, modulo (var % stack), push
    LoadAddI64Slot(usize),
    LoadSubI64Slot(usize),
    LoadMulI64Slot(usize),
    LoadModI64Slot(usize),

    // Store-Op fusion: Optimized variable updates
    IncVarI64(String), // Pop stack, add to variable, store back
    DecVarI64(String), // Pop stack, subtract from variable, store back
    IncVarI64Slot(usize),
    DecVarI64Slot(usize),

    // Compare-Jump fusion: Comparison + conditional jump
    JumpIfNeI64(usize), // Pop 2 I64s, jump if not equal
    JumpIfEqI64(usize), // Pop 2 I64s, jump if equal
    JumpIfLtI64(usize), // Pop 2 I64s, jump if less than
    JumpIfGtI64(usize), // Pop 2 I64s, jump if greater than
    JumpIfLeI64(usize), // Pop 2 I64s, jump if less or equal
    JumpIfGeI64(usize), // Pop 2 I64s, jump if greater or equal

    // ★ Int64 comparisons
    GtI64, // (a > b) -> I64(0/1)
    LtI64, // (a < b) -> I64(0/1)
    LeI64, // (a <= b) -> I64(0/1)
    GeI64, // (a >= b) -> I64(0/1)
    EqI64, // (a == b) -> I64(0/1)
    NeI64, // (a != b) -> I64(0/1)

    // Int -> Float conversion
    ToF64, // pop I64, push F64
    // Float -> Int conversion (truncate)
    ToI64, // pop F64, push I64
    // Bool -> Int conversion
    BoolToI64, // pop Bool, push I64 (true -> 1, false -> 0)
    // Int -> Bool conversion (0 = false, non-zero = true)
    I64ToBool, // pop I64, push Bool
    // Bool negation
    NotBool, // pop Bool, push !Bool
    // Dynamic type conversions (for Any typed values)
    DynamicToF64,  // pop Any value, convert to F64 (handles I64 -> F64, F64 -> F64)
    DynamicToF32,  // pop Any value, convert to F32 (handles I64 -> F32, F64 -> F32, F32 -> F32)
    DynamicToF16,  // pop Any value, convert to F16 (handles I64 -> F16, F64 -> F16, F16 -> F16)
    DynamicToI64,  // pop Any value, convert to I64 (handles F64 -> I64, I64 -> I64)
    DynamicToBool, // pop Any value, check it's Bool (else TypeError)
    // Small integer back-conversion (Issue #2278): convert I64 result back to small integer type
    DynamicToI8,  // pop I64 value, truncate to I8
    DynamicToI16, // pop I64 value, truncate to I16
    DynamicToI32, // pop I64 value, truncate to I32
    DynamicToU8,  // pop I64 value, truncate to U8
    DynamicToU16, // pop I64 value, truncate to U16
    DynamicToU32, // pop I64 value, truncate to U32
    DynamicToU64, // pop I64 value, truncate to U64

    // Float64 ops
    AddF64,
    SubF64,
    MulF64,
    DupF64, // duplicate top F64 on stack
    DivF64,
    SqrtF64, // sqrt(x) - CPU instruction
    // NOTE: sin, cos, tan, asin, acos, atan, exp, log, round are now Builtins (Layer 2)
    FloorF64, // floor(x) - CPU instruction
    CeilF64,  // ceil(x) - CPU instruction
    AbsF64,   // abs(x) - CPU instruction
    Abs2F64,  // abs2(x) = x^2
    SleepF64, // sleep(secs) where secs is Float64
    SleepI64, // sleep(secs) where secs is Int64
    PowF64,   // pow(base, exp)
    NegF64,
    // Float64 comparisons -> I64(0/1)
    LtF64,
    GtF64,
    LeF64,
    GeF64,
    EqF64,
    NeF64,

    // Struct field comparison (default == for immutable structs)
    // Compares all fields recursively using ==
    // Returns Bool(true) if same struct type and all fields equal
    EqStruct,

    // String equality comparison
    // Pops two strings, pushes Bool(true) if equal
    EqStr,

    // String ordered comparisons (lexicographic)
    // Pop two strings, push Bool result (Issue #2025)
    LtStr,
    LeStr,
    GtStr,
    GeStr,

    // control
    SelectI64, // pops else, then, cond -> pushes chosen
    SelectF64,

    // RNG
    RandF64,
    RandArray(usize),    // Pop N dims, create array of random Float64 in [0, 1)
    RandIntArray(usize), // Pop N dims, create array of random Int64
    RandnF64,            // randn() with global RNG - standard normal distribution
    RandnArray(usize),   // Pop N dims, create array of randn Float64 with global RNG
    SeedGlobalRng,       // Pop I64, reseed global RNG

    // flow
    Jump(usize),
    JumpIfZero(usize),  // pop I64, if 0 jump
    Call(usize, usize), // (func_index, arg_count)
    /// Call with @inbounds context enabled in the callee frame.
    CallInbounds(usize, usize), // (func_index, arg_count)
    /// Call with keyword arguments: (func_index, pos_arg_count, kwarg_names)
    /// Expects pos_arg_count positional args followed by kwarg_names.len() kwarg values on stack
    CallWithKwargs(usize, usize, Vec<String>),
    /// Call with keyword arguments and splat expansion: (func_index, pos_arg_count, kwarg_names, kwargs_splat_mask)
    /// Expects pos_arg_count positional args followed by kwarg_names.len() kwarg values on stack
    /// kwargs_splat_mask[i] == true means kwargs[i] should be expanded (e.g., from NamedTuple)
    CallWithKwargsSplat(usize, usize, Vec<String>, Vec<bool>),
    /// Call with splat expansion: (func_index, arg_count, splat_mask)
    CallWithSplat(usize, usize, Vec<bool>),
    /// Call a core intrinsic function (e.g., add_int, mul_float).
    /// These are the lowest-level operations that map directly to CPU instructions.
    CallIntrinsic(Intrinsic),
    /// Call a builtin function (e.g., sin, cos, map, filter).
    /// These are library functions implemented in Rust, one layer above intrinsics.
    /// The usize parameter is the argument count.
    CallBuiltin(BuiltinId, usize),
    /// Runtime method dispatch for multiple dispatch support.
    /// When compile-time type is Any but multiple methods exist, this instruction
    /// allows VM to select the best matching method based on actual argument types.
    /// The boxed payload retains the compiler-resolved callee identity alongside
    /// the fallback index, argument count, and candidates (Issue #10461).
    ///
    /// Issue #6496: candidates are structured [`DynamicCallCandidate`]s —
    /// `Method(func_index)` for real (single-parameter) method candidates,
    /// whose expected type name the runtime derives from `FunctionInfo`, and
    /// `NativeIterator(kind)` for the VM-native collect boundary (previously
    /// the `(usize::MAX, name)` sentinel encoding).
    CallDynamic(Box<DynamicCallOperands>),

    /// Runtime method dispatch for binary operators when one operand has Any type.
    /// Format: (fallback_func_index, check_position, candidate_func_indices)
    /// - check_position: which argument position (0 or 1) has the Any type to check
    /// At runtime, VM checks the type of arg[check_position] against each
    /// candidate's parameter type at that position and dispatches.
    ///
    /// Issue #6496: candidates carry only the function index; the expected
    /// type name at `check_position` is derived from the candidate's
    /// `FunctionInfo` (previously a baked `(func_index, name)` string pair).
    CallDynamicBinary(usize, usize, Vec<usize>),

    /// Runtime dispatch for binary operators when both operands have Any type.
    /// Format: (fallback_intrinsic, candidate_func_indices)
    /// - fallback_intrinsic: The intrinsic to use if both operands are primitives
    /// At runtime, VM checks both operand types and dispatches to matching method.
    /// If no method matches and both are primitives, uses the fallback intrinsic.
    ///
    /// Issue #6496: candidates carry only the function index; the
    /// `(left, right)` expected type names are derived from each candidate's
    /// `FunctionInfo` and memoized per function index on the `Vm`
    /// (previously baked `(func_index, left_name, right_name)` triples).
    CallDynamicBinaryBoth(Intrinsic, Vec<usize>),

    /// Runtime dispatch for binary operators WITHOUT builtin fallback.
    /// Used when user-defined methods shadow builtins completely.
    /// Format: candidate function indices (Issue #6496: `(left, right)`
    /// expected type names are derived from each candidate's `FunctionInfo`).
    /// At runtime, VM matches operand types to candidates and calls matching method.
    /// Returns MethodError if no matching method is found.
    CallDynamicBinaryNoFallback(Vec<usize>),

    /// Runtime dispatch for unary functions with builtin fallback.
    /// Format: (builtin_id, candidate_func_indices)
    /// - builtin_id: The builtin to call if argument is not a struct type
    /// At runtime, if the argument matches a candidate's first parameter type
    /// (derived from `FunctionInfo`, Issue #6496; previously a baked name
    /// string), calls the user method. Otherwise, converts to F64 and calls
    /// the builtin.
    CallDynamicOrBuiltin(BuiltinId, Vec<usize>),

    /// Runtime typed dispatch with a builtin fallback for multi-argument public
    /// Base routes that still retain a VM primitive fallback.
    /// Format: (builtin_id, function_name, arg_count, candidate_func_indices)
    /// At runtime, all argument types are scored against matching-arity
    /// candidates (expected type names derived from each candidate's
    /// `FunctionInfo`, Issue #6496). If none match, the original arguments are
    /// passed to the builtin fallback.
    CallTypedDispatchOrBuiltin(BuiltinId, String, usize, Vec<usize>),

    /// Runtime typed dispatch with a builtin fallback that leaves a separate
    /// result value. Used by Array `pop!`-style fallbacks whose builtin returns
    /// `[modified_collection, result]`; the collection is already mutated by
    /// reference, so only the result remains on the stack.
    CallTypedDispatchOrBuiltinResult(BuiltinId, String, usize, Vec<usize>),

    /// Runtime typed dispatch with a Dict-mutating builtin fallback.
    /// Same as `CallTypedDispatchOrBuiltin`, but when the builtin fallback wins,
    /// the resulting Dict is stored back into the named local before being
    /// returned.
    CallTypedDispatchOrBuiltinStoreDict(Box<TypedDispatchStoreDict>),

    /// Runtime typed dispatch with a Dict-mutating builtin fallback that
    /// leaves a separate result value. Used by `pop!`, whose fallback returns
    /// `[modified_collection, popped_value]`.
    CallTypedDispatchOrBuiltinStoreDictResult(Box<TypedDispatchStoreDict>),

    /// Dynamic dispatch for iterate() with 1 or 2 arguments.
    /// Used when collection type is Any at compile time (e.g., parametric struct field).
    /// Format: (argc, candidates)
    /// - argc: Number of arguments (1 for initial, 2 for subsequent)
    /// - candidates: candidate `iterate` method function indices
    ///
    /// Issue #6336: the payload is the structured candidate set only — the
    /// historical `\u{1f}`-joined type-name signature strings are gone; the
    /// runtime name-pattern fallback derives the per-arity signature from each
    /// candidate's `FunctionInfo` instead of deserializing baked name strings.
    /// At runtime:
    /// 1. Check if first arg (collection) is StructRef
    /// 2. If StructRef, find matching iterate method in candidates and call it
    /// 3. If not struct, use builtin iterate (IterateFirst/IterateNext)
    IterateDynamic(usize, Vec<usize>),

    /// Runtime dispatch for Type{T} patterns.
    /// Used when calling functions with ::Type{T} parameter patterns and the specific
    /// type values aren't known at compile time (e.g., promote_rule(T, S) where T,S are type params).
    /// Format: (func_name, arg_count, fallback_index, candidates)
    /// - func_name: function name for error messages
    /// - arg_count: number of arguments
    /// - fallback_index: function index to call if no specific match (generic where clause version)
    /// - candidates: candidate method function indices. The per-arity expected
    ///   type names (e.g. `["Float64", "Int64"]`) are derived from each
    ///   candidate's `FunctionInfo` at runtime (Issue #6496); equality with the
    ///   historical baked strings is pinned by the parity gate in
    ///   `compile/cache.rs`.
    /// At runtime, pops DataType values from stack and matches against candidates.
    CallTypedDispatch(String, usize, usize, Vec<usize>),

    /// Direct call where the callee's `where` parameters are supplied by a
    /// static parametric callable surface such as `M{2, 2}(args...)`.
    /// The ordinary positional args are popped from the stack; `bindings` are
    /// installed into the callee frame before its body runs.
    CallStaticParametric(Box<StaticParametricCall>),

    /// Runtime candidate selection over an explicit-parametric constructor
    /// family (`Foo{T}(x::Int) where T` / `Foo{S}(x::String) where S`, ...) by
    /// value signature, with a per-candidate `where`-binder payload installed
    /// into the selected frame. Fixes Issue #10971: the explicit type
    /// argument(s) are compile-time literals (`Foo{Int}(x)`) but the *value*
    /// argument is runtime-unknown, so candidate selection itself must happen
    /// at runtime — each candidate's `bindings` carries its own baked binder
    /// (binder names may differ per candidate, e.g. `T` vs `S`). The payload
    /// also supports a genuinely runtime type-argument VALUE (popped from the
    /// stack above the positional args and bound under each candidate's own
    /// `runtime_binding_names`) for a future call site; no current emission
    /// site uses that path — Issue #10968's sibling case (the type argument
    /// itself is a runtime `DataType` local) was found already fixed on main
    /// and does not emit this instruction.
    /// Stack layout: `[args..., type_arg_values...]` (args below, type-arg
    /// values on top, mirroring `CallStaticParametric`'s
    /// `runtime_binding_names` convention).
    CallParametricConstructorDispatch(Box<ParametricConstructorDispatchCall>),

    /// Dynamic type constructor call: T(x) where T is a DataType value.
    /// Pops the DataType and the value from stack, converts the value to that type.
    /// Used for patterns like: T = Float64; T(1) or function f(T, x); T(x); end
    CallTypeConstructor,

    /// Call a GlobalRef as a function: ref(args...) where ref is a GlobalRef.
    /// Stack layout: [args..., globalref] where globalref is a GlobalRef value
    /// (arg_count) - number of arguments on stack below the GlobalRef
    /// At runtime:
    /// 1. Pop the GlobalRef from stack
    /// 2. Pop arg_count arguments from stack
    /// 3. Resolve the GlobalRef to the actual function (by module.name)
    /// 4. Call the resolved function with the arguments
    /// Example: ref = GlobalRef(Base, :println); ref("hello")
    ///   -> stack has ["hello", GlobalRef(Base, :println)], instr is CallGlobalRef(1)
    CallGlobalRef(usize),

    /// Call a function stored in a local variable.
    /// This handles patterns like: function setprecision(f::Function, ...); f(); end
    /// 1. Pop the Function value from stack (top of stack)
    /// 2. Pop arg_count arguments from stack
    /// 3. Call the function with the arguments
    /// arg_count is the number of arguments (not including the function itself)
    CallFunctionVariable(usize),

    /// Invoke a function value using an explicit declared signature rather than
    /// runtime argument dispatch. Stack layout: [args..., function_value].
    InvokeFunctionVariable(usize, Vec<String>),

    /// Invoke a function value using an explicit declared signature and keyword
    /// arguments. Stack layout: [args..., kwarg_values..., function_value].
    ///
    /// Boxed to keep `Instr` compact (Issue #5176).
    InvokeFunctionVariableWithKwargs(Box<InvokeWithKwargs>),

    /// Invoke a function value using a runtime Tuple signature value. Stack
    /// layout: [args..., function_value, signature_value].
    InvokeFunctionVariableDynamicSignature(usize),

    /// Invoke a function value using a runtime Tuple signature value and
    /// keyword arguments. Stack layout: [args..., kwarg_values...,
    /// function_value, signature_value].
    InvokeFunctionVariableDynamicSignatureWithKwargs(usize, Vec<String>, Vec<bool>),

    /// Call a function variable with splatted arguments.
    /// This handles patterns like: function apply_variadic(f, args...); f(args...); end
    /// 1. Pop the Function value from stack (top of stack)
    /// 2. Pop arg_count arguments from stack
    /// 3. Expand splatted arguments based on splat_mask
    /// 4. Call the function with the expanded arguments
    /// (arg_count, splat_mask)
    CallFunctionVariableWithSplat(usize, Vec<bool>),

    /// Call a function variable with keyword arguments and optional splats.
    /// Stack layout: [pos_args..., kwarg_values..., function_value].
    /// The positional splat mask applies to pos_args; keyword splat mask applies
    /// to kwarg_values where the corresponding kwarg name is a placeholder.
    ///
    /// Boxed to keep `Instr` compact (Issue #5176).
    CallFunctionVariableWithKwargsSplat(Box<CallVarKwargsSplat>),

    /// Lazy AoT call: specialize function based on runtime argument types.
    /// (specializable_func_index, arg_count)
    ///
    /// At runtime:
    /// 1. Pop arg_count arguments from stack
    /// 2. Extract actual ValueTypes from arguments (typeof(x))
    /// 3. Look up (func_index, arg_types) in specialization cache
    /// 4. If cache miss: compile specialized bytecode, append to code vector, cache entry
    /// 5. Call the specialized function entry point
    CallSpecialize(usize, usize),
    /// Lazy AoT call entered from an `@inbounds` context.
    ///
    /// This follows `CallSpecialize`, but marks the callee frame so lowered
    /// `@boundscheck` guards can observe that checks are disabled.
    CallSpecializeInbounds(usize, usize),

    // return
    ReturnI64,
    ReturnF64,
    ReturnF32,
    ReturnF16,
    ReturnArray,
    ReturnNothing,
    ReturnAny, // Dynamic return - pops and returns whatever is on stack

    // stack manipulation
    Pop,     // Pop and discard top of stack
    PopIfIO, // Pop if IO, otherwise leave on stack (for runtime IO detection in print)
    Swap,    // Swap top two values on stack

    // I/O
    PrintStr,          // pop Str, print with newline
    PrintI64,          // pop I64, print with newline
    PrintF64,          // pop F64, print with newline
    PrintStrNoNewline, // pop Str, print without newline
    PrintI64NoNewline, // pop I64, print without newline
    PrintF64NoNewline, // pop F64, print without newline
    PrintAny,          // pop any type, print with newline
    PrintAnyNoNewline, // pop any type, print without newline
    PrintNewline,      // print newline
    // Issue #4853 follow-up: pop one value and write only a trailing newline to
    // it as a sink. If the value is an `IO` (IOBuffer/stdout/stderr/devnull),
    // the newline goes to that sink; otherwise (a non-IO first arg of a
    // `println(first, x)`) the newline goes to stdout. Used by the runtime-
    // discriminating `println(io_or_val, x)` single-value lowering so that a
    // local-`IOBuffer()` first arg reaches user `show` then gets its newline on
    // the same sink, without double-printing the first value.
    IOPrintlnNewline,

    // String operations
    ToString,            // pop any value, push its string representation
    StringConcat(usize), // pop N values, concatenate as strings, push result

    // Errors
    ThrowError,         // pop String, throw ErrorException
    ThrowValue,         // pop any Value (e.g., exception struct), throw it as exception
    PushExceptionValue, // push the pending exception value onto stack (for catch blocks)

    // Test macros
    Test(String),            // pop I64, print pass/fail (does not error on fail)
    TestSetBegin(String),    // begin a test set with name
    TestSetEnd,              // end current test set and print summary
    TestThrowsBegin(String), // begin test_throws with expected exception type
    TestThrowsEnd,           // end test_throws - check if expected exception was thrown

    // Time
    TimeNs, // push current time in nanoseconds as I64

    // Array operations
    NewArray(usize),                          // Create empty array with capacity
    PushElem,                                 // Pop value, push to array on stack
    FinalizeArray(Vec<usize>),                // Set shape, finalize array
    PushArrayValue(Box<ArrayLiteralPayload>), // Push array literal directly (boxed; Issue #5176)

    LoadArray(String),  // Load array from local variable
    StoreArray(String), // Store array to local variable

    IndexLoad(usize),          // Pop N indices, pop array, push element
    IndexLoadInbounds(usize),  // Pop N compiler-proven in-bounds indices, pop array, push element
    IndexSlice(usize),         // Pop N indices, pop array, push subarray
    IndexStore(usize),         // Pop value, pop N indices, pop array, store, push array
    IndexStoreInbounds(usize), // Pop value, pop N compiler-proven in-bounds indices, pop array, store, push array

    // NOTE: ArrayLen, ArraySum, ArrayShape, ArrayToSizeTuple, Zeros, Ones, Trues, Falses, Fill
    //       have been moved to CallBuiltin (Layer 2 Builtins)
    Zero, // Pop value, push zero of same type (0.0 for f64, 0+0im for complex)

    ArrayPush, // Pop value, pop array, push to array, push array
    /// Pop value, pop array, push with collect-style typejoin, push array.
    /// Used for size-unknown filtered comprehensions where Julia discovers
    /// element type from produced values instead of converting to Float64.
    ArrayPushTypejoin,
    /// Pop an I64 element count, reserve that much capacity on the array left on
    /// the stack, then leave the array on the stack. Used to pre-size the
    /// backing storage of a filter-free comprehension whose final length is
    /// already known at runtime, avoiding the O(log n) reallocations of repeated
    /// `ArrayPush` growth (Issue #5186). A pure capacity hint: a count <= 0 or a
    /// non-Array operand is a no-op so the instruction never affects results.
    ReserveArray,
    ArrayPop,       // Pop array, pop last element, push array, push element
    ArrayPushFirst, // Pop value, pop array, prepend to array, push array
    ArrayPopFirst,  // Pop array, pop first element, push array, push element
    ArrayInsert,    // Pop value, pop index, pop array, insert at index, push array
    ArrayDeleteAt,  // Pop index, pop array, delete at index, push array

    // === Typed Array Operations (for heterogeneous arrays) ===
    /// Create empty typed array with element type and capacity
    /// Element type determines storage format (e.g., Any for heterogeneous)
    NewArrayTyped(ArrayElementType, usize),
    /// Push value to typed array on stack
    /// VM will validate type compatibility based on array's element type
    PushElemTyped,
    /// Load element from typed array, returning Value
    /// Pop N indices, pop array, push element as Value
    IndexLoadTyped(usize),
    /// Load element from typed array when the compiler proved the index is in-bounds.
    /// Pop N indices, pop array, push element as Value.
    IndexLoadTypedInbounds(usize),
    /// Store Value to typed array
    /// Pop value, pop N indices, pop array, store, push array
    IndexStoreTyped(usize),
    /// Finalize typed array with shape
    FinalizeArrayTyped(Vec<usize>),

    /// Allocate uninitialized typed array: pop `argc` dimension values, create Array{T}(undef, dims...)
    /// Handles all element types generically (Issue #2218)
    AllocUndefTyped(ArrayElementType, usize),

    /// Allocate uninitialized typed array from one tuple of dimensions.
    /// Mirrors Julia's `Array{T,N}(undef, dims::NTuple{N,Int})` constructor
    /// family by unpacking tuple dims before allocation.
    AllocUndefTypedFromTuple(ArrayElementType),

    /// Allocate uninitialized typed array where the element type is a runtime DataType.
    /// Stack layout (top → bottom): dim_n, dim_(n-1), ..., dim_1, DataType.
    /// Used to compile `Vector{T}(undef, n)` / `Array{T}(undef, dims...)` when `T`
    /// is a local DataType variable (e.g., `T = eltype(arr)`). (Issue #3648)
    AllocUndefDynamicTyped(usize),

    /// Runtime-typed version of `AllocUndefTypedFromTuple`.
    /// Stack layout (top → bottom): dims tuple, DataType.
    AllocUndefDynamicTypedFromTuple,

    MatMul, // Pop B, pop A, push A * B (matrix/vector multiplication)
    // Note: Adjoint and Transpose have been migrated to Pure Julia
    // See: subset_julia_vm/src/julia/base/array.jl
    MakeRange,    // Pop stop, pop step, pop start (I64), create Int64 range array
    MakeRangeF64, // Pop stop, pop step, pop start (F64), create Float64 range array

    // Lazy range operations
    MakeRangeLazy, // Pop stop, pop step, pop start, create lazy Range value (does not materialize)
    LoadRange(String), // Load range from local variable
    StoreRange(String), // Store range to local variable
    ReturnRange,   // Return range value
    RangeCollect,  // Pop range, materialize to array, push array
    RangeFirst,    // Pop range, push first element as F64
    RangeLast,     // Pop range, push last element as F64
    RangeGetIndex, // Pop index (I64), pop range, push element as F64

    // String operations (PushStr is defined above with PushI64/PushF64)
    ConcatStrings(usize), // Pop N strings, concatenate them, push result
    ToStr,                // Pop any value, convert to string, push

    // Try/catch/finally support
    PushHandler(Option<usize>, Option<usize>), // (catch_ip, finally_ip)
    PopHandler,
    ClearError,
    PushErrorCode,
    PushErrorMessage,
    Rethrow,
    /// Rethrow the current pending exception unconditionally.
    /// Used for Julia's rethrow() function.
    RethrowCurrent,
    /// Rethrow with a new exception value from top of stack.
    /// Used for Julia's rethrow(e) function.
    RethrowOther,

    // Slice marker (:) for indexing
    SliceAll,

    // Struct operations
    NewStruct(usize, usize), // (type_id, field_count) - pop N values, create struct instance
    NewStructSplat(usize),   // (type_id) - pop tuple/array, unpack into struct fields
    /// Create a parametric struct by resolving type bindings from the current frame.
    /// (base_name, field_count) - e.g., ("Rational", 2) with T=Int64 in frame creates Rational{Int64}
    NewParametricStruct(String, usize),
    /// Create a parametric struct with dynamically resolved type parameters.
    /// (base_name, field_count, type_param_count)
    /// Stack layout: [field_values..., type_params...] where type_params are DataType values
    /// Example: Point{Tnew}(x, y) where Tnew = Float64
    ///   -> stack has [x, y, Float64], instr is NewDynamicParametricStruct("Point", 2, 1)
    NewDynamicParametricStruct(String, usize, usize),
    /// Construct a parametric type from base name and type arguments on stack.
    /// Stack layout: [type_arg1, type_arg2, ...] (all DataType values)
    /// Pushes the resulting DataType onto stack.
    /// Example: Complex{promote_type(T, S)} where T=Float64, S=Int64
    ///   -> stack has [Float64], after evaluating promote_type
    ///   -> ConstructParametricType("Complex", 1) pops Float64, pushes Complex{Float64} DataType
    ConstructParametricType(String, usize), // (base_name, num_type_args)
    LoadStruct(String),            // Load struct from local variable
    StoreStruct(String),           // Store struct to local variable
    GetField(usize),               // (field_index) - pop struct, push field value
    GetFieldByName(String), // (field_name) - pop struct, look up field by name at runtime, push value
    SetField(usize),        // (field_index) - pop value, pop struct, set field, push struct
    SetFieldByName(String), // (field_name) - pop value, pop struct, look up field by name, set, push struct
    GetExprField(usize),    // (0=head, 1=args) - pop Expr, push field value (Symbol or Array)
    GetLineNumberNodeField(usize), // (0=line, 1=file) - pop LineNumberNode, push field value
    GetQuoteNodeValue,      // pop QuoteNode, push inner value
    GetGlobalRefField(usize), // (0=mod, 1=name) - pop GlobalRef, push field value
    ReturnStruct,           // Return struct value

    // Higher-order function operations
    // Note: MapFunc, FilterFunc, ReduceFunc, FoldrFunc removed - now Pure Julia (base/iterators.jl)
    // Note (Issue #6733): the legacy predicate/reducer HOF instructions
    // FindAllFunc / FindFirstFunc / FindLastFunc / MapReduceFunc(WithInit) /
    // MapFoldrFunc(WithInit) / MapFuncInPlace / FilterFuncInPlace / SumFunc /
    // AnyFunc / AllFunc / CountFunc were removed. findall/findfirst/findlast,
    // mapreduce/mapfoldr, map!/filter!, sum/any/all/count are now pure Julia
    // (base/reduce.jl, base/iterators.jl, base/array.jl) and resolve through
    // normal method dispatch; these instructions were no longer emitted.
    /// ntuple(f, n) - Create tuple by calling f(i) for i in 1:n
    NtupleFunc(usize), // func_index

    /// ntuple(f, n) where f is a runtime callable value.
    /// Pop n, pop callable, push tuple.
    NtupleRuntime,

    /// Generator(f, iter) - Create lazy generator.
    /// Pop iterator, push Generator with callable and optional default
    /// result element type for empty collection.
    MakeGenerator(Box<MakeGeneratorOperands>), // (callable, result_element_type); boxed (Issue #5176)

    /// Generator(f, iter) where f is a runtime value.
    /// Pop iterator, pop callable, push Generator.
    MakeGeneratorRuntime(bool, Option<ArrayElementType>), // (tuple_splat, result_element_type)

    /// Wrap an array in a Generator for eager-evaluated generator expressions
    /// Pop Array, push Generator wrapping that array
    WrapInGenerator,

    /// sprint(f, args...) - Call f(io, args...) and return the result as a string
    /// Pop N args, create IOBuffer, call function with IOBuffer + args, push string
    SprintFunc(usize, usize), // (func_index, arg_count)

    // RNG instance operations
    NewStableRng,            // pop seed(I64), push Rng(StableRng)
    NewXoshiro,              // pop seed(I64), push Rng(Xoshiro)
    NewMersenne,             // pop seed(I64), push Rng(MersenneTwister) — Issue #7306
    LoadRng(String),         // load RNG from local variable
    StoreRng(String),        // store RNG to local variable
    RngRandF64,              // pop Rng, push F64, push Rng (mutated)
    RngRandArrayF64(usize),  // pop dims, pop Rng, push Array[F64], push Rng
    RngRandArrayI64(usize),  // pop dims, pop Rng, push Array[I64], push Rng
    RngRandnF64,             // pop Rng, push randn() F64, push Rng
    RngRandnArrayF64(usize), // pop dims, pop Rng, push randn Array[F64], push Rng
    ReturnRng,               // Return RNG value
    // Push a handle to the VM's global RNG (Random.default_rng()/GLOBAL_RNG):
    // Value::Rng(RngInstance::Global). Drawing through it advances the SAME
    // stream as bare rand()/randn() (Issue #7230).
    PushGlobalRng,
    // Single-argument `randn(x)` / `rand(x)` where `x` is statically untyped
    // (ValueType::Any): pop the value; if it is a Value::Rng produce a scalar
    // randn/rand from that RNG (writing the advanced state back to the named
    // local when Some), otherwise treat it as a dimension and produce a vector
    // (matching `randn(n)` / `rand(n)`). Resolves an untyped RNG-carrying param
    // `f(rng) = randn(rng)` (Issue #7231).
    RandnArg(Option<String>),
    RandArg(Option<String>),

    // Core.SimpleVector (svec) construction (Issue #4722).
    MakeSimpleVector(usize), // Pop N values, create a Core.SimpleVector, push

    // Tuple operations
    NewTuple(usize),    // Pop N values, create tuple, push
    LoadTuple(String),  // Load tuple from local
    StoreTuple(String), // Store tuple to local
    TupleGet,           // Pop index(I64), pop tuple, push element
    // NOTE: TupleLen, TupleFirst, TupleLast moved to CallBuiltin (Layer 2)
    TupleUnpack(usize), // Pop tuple, push N elements (destructuring)
    ReturnTuple,        // Return tuple value

    // NamedTuple operations
    NewNamedTuple(Vec<String>), // Pop N values, create named tuple with field names
    LoadNamedTuple(String),     // Load named tuple from local
    StoreNamedTuple(String),    // Store named tuple to local
    NamedTupleGetField(String), // Pop named tuple, push field by name
    NamedTupleGetIndex,         // Pop index(I64), pop named tuple, push element
    NamedTupleGetBySymbol,      // Pop symbol, pop named tuple, push field by symbol name
    ReturnNamedTuple,           // Return named tuple value

    // Base.Pairs operations (for kwargs...)
    NewPairs(Vec<String>), // Pop N values, create Pairs with field names
    PairsGetBySymbol,      // Pop symbol, pop pairs, push field by symbol name
    PairsLength,           // Pop pairs, push length as I64
    PairsKeys,             // Pop pairs, push keys as tuple of symbols
    PairsValues,           // Pop pairs, push values as NamedTuple

    // Dict operations. `Value::Dict` was retired (Issue #6731), so the
    // carrier-construction instructions (`NewDict`/`NewDictWithPairs`/
    // `NewDictTyped`) were removed; the rest are now Set-shared (Issue #1832).
    LoadDict(String),  // Load Set from local (Dict locals are StructRef)
    StoreDict(String), // Store Set to local
    DictSet,           // Unreachable after Value::Dict removal (kept for bytecode shape)
    DictLen,           // Pop Set, push length as I64
    // NOTE: DictGet, DictGetDefault, DictHasKey, DictDelete, DictKeys,
    //       DictValues, DictPairs, DictMerge moved to CallBuiltin (Layer 2)
    ReturnDict, // Return dict value

    // Set operations
    NewSet,              // Create empty set, push
    NewSetTyped(String), // Create empty set with element type param, push
    SetAdd,              // Pop element, pop set, add element, push set
    StoreSet(String),    // Store set to local
    LoadSet(String),     // Load set from local
    ReturnSet,           // Return set value

    // Ref operations (broadcast protection)
    MakeRef,   // Pop value, wrap in Ref, push Ref
    UnwrapRef, // Pop Ref, push inner value
    ReturnRef, // Return Ref value

    // ForEach iteration
    IterateFirst, // Pop iterable, push (element, state) or Nothing
    IterateNext,  // Pop state, pop iterable, push (element, state) or Nothing
    IsNothing,    // Pop value, push I64 1 if Nothing, 0 otherwise
    TupleFirst,   // Pop tuple, push first element
    TupleSecond,  // Pop tuple, push second element

    // Issue #5168: tuple-free ForEach iteration. Instead of materializing a
    // `(element, state)` 2-tuple on the heap every iteration, these push the
    // results split across the stack:
    //   - has value: push `state`, push `element`, push `Bool(true)` (flag on top)
    //   - exhausted: push `Bool(false)` (flag only)
    // The ForEach lowering then `JumpIfZero`s on the flag (which pops it) and,
    // when non-zero, finds `[state, element]` on the stack to store directly,
    // avoiding the per-iteration tuple allocation and the TupleFirst/TupleSecond
    // clones. Builtin fast-path collections (Array/Memory/Range/Tuple/String)
    // skip the tuple entirely; all other collections delegate to the existing
    // tuple-returning `iterate_first`/`iterate_next` and are unpacked on the stack.
    IterateFirstSplit, // Pop iterable, push split (state, element, flag)
    IterateNextSplit,  // Pop state, pop iterable, push split (state, element, flag)

    // Memory{T} operations
    /// Create a new Memory{T} with given element type and length (undef-initialized).
    /// Pushes Memory value onto stack.
    NewMemory(ArrayElementType, usize),
    /// Create a new Memory{T} with given element type and dynamic length from stack.
    /// Pops I64 length from stack, pushes Memory value onto stack.
    NewMemoryDynamic(ArrayElementType),
    /// Create a new Memory{T} where T is a runtime DataType value.
    /// Pops I64 length and DataType element type from stack, pushes Memory value.
    NewMemoryDynamicTyped,
    /// Pop index (I64), pop Memory, push element at index (1-indexed).
    MemoryGet,
    /// Pop value, pop index (I64), pop Memory, set element at index (1-indexed), push Memory back.
    MemorySet,
    /// Pop Memory, push its length as I64.
    MemoryLength,
    /// Load Memory from local variable by name.
    LoadMemory(String),
    /// Store top-of-stack Memory into local variable by name.
    StoreMemory(String),
    /// Return Memory value from function.
    ReturnMemory,

    // Variable reflection
    /// Check if a variable is defined in the current scope.
    /// Pushes Bool(true) if defined, Bool(false) otherwise.
    IsDefined(String),

    /// Compile-time resolved direct call.
    ///
    /// Appended near the end of the enum (instead of beside `Call`) to preserve
    /// existing bincode discriminants for serialized bytecode/cache entries.
    CallResolved(usize, usize),

    /// Slotized Symbol local access.
    ///
    /// Appended near the end of the enum to preserve existing bincode
    /// discriminants for serialized bytecode/cache entries.
    LoadSlotSymbol(usize),
    StoreSlotSymbol(usize),

    /// No-operation placeholder used during instruction dispatch (Issue #2939).
    /// Never appears in compiled bytecode; only used transiently in the execution loop
    /// to avoid cloning instructions on every cycle.
    Nop,

    /// Throw a catchable runtime `MethodError` carrying the inline message.
    ///
    /// Emitted for an ambiguous method call that has no most-specific
    /// resolution and no runtime-dispatch fallback (Issue #5071). Upstream
    /// Julia raises an ambiguity `MethodError` at runtime (catchable via
    /// try/catch and `@test_throws MethodError`); previously sjulia aborted
    /// compilation with `CompileError::Dispatch(AmbiguousMethod{..})`.
    ///
    /// Unlike `ThrowError` (which pops a `String` and throws `ErrorException`),
    /// the message is known at compile time and carried inline so the runtime
    /// raises `VmError::MethodError` directly. Appended at the very end of the
    /// enum to preserve every existing bincode discriminant.
    ThrowMethodError(String),

    /// Construct a parametric type whose type arguments may be `...`-splatted
    /// collections (`Tuple{xs...}`, `Core.apply_type(base, args...)`).
    ///
    /// Stack layout mirrors [`Instr::CallWithSplat`]: the `splat_mask.len()`
    /// argument values are pushed in source order (oldest deepest). For each
    /// `i`, `splat_mask[i] == true` means `args[i]` is a splatted collection
    /// (`Tuple`/`Array`/`SimpleVector`/`Range`) to flatten in place via
    /// `expand_splat_arguments`; the flattened list is then handed to the same
    /// construction logic as [`Instr::ConstructParametricType`] (Issue #5112).
    ///
    /// Appended at the very end of the enum to preserve every existing bincode
    /// discriminant for the persistent Base cache.
    ConstructParametricTypeSplat(String, Vec<bool>),

    /// `Core.apply_type(base, params...)` where the base type is only known at
    /// runtime (e.g. a local variable holding a `DataType`), so it cannot be
    /// resolved to a static base-name string at compile time (Issue #5112).
    ///
    /// Stack layout: `[base, param_1, ..., param_n]` with `base` deepest. The
    /// base value's canonical `TypeName` supplies the base name, then the same
    /// construction logic as [`Instr::ConstructParametricType`] runs over the
    /// `n` parameters. The operand is `n` (the parameter count, excluding the
    /// base). Appended at the end of the enum to preserve bincode discriminants.
    ApplyTypeDynamic(usize),

    /// Register an `@enum` type and its members in the thread-local runtime enum
    /// registry (Issue #5139). Emitted once per `@enum` definition, before the
    /// member-binding `PushEnum`/`StoreAny` pairs. Populates the value->name and
    /// declaration-order tables used for display, `Color(v)` construction, and
    /// `instances(Color)`. The operands are boxed so the wide `(String, Vec)`
    /// payload does not dictate the dense `code` array's stride (Issue #5176).
    /// Appended at the end of the enum to preserve bincode discriminants.
    RegisterEnum(Box<RegisterEnumOperands>),

    /// Construct an enum value from an integer: `Color(value)` (Issue #5139).
    /// Pops the integer value, validates it against the registered members of
    /// `type_name`, and pushes `Value::Enum`; an invalid value raises an
    /// `ArgumentError`, matching upstream. Appended at the end of the enum to
    /// preserve bincode discriminants.
    ConstructEnum(String),
    /// Like `MakeRangeLazy`, but for the explicit-step form `a:s:b` — the result is
    /// a `StepRange` even when the step is 1 (`1:1:5`), distinguishing it from the
    /// `UnitRange` `1:5` (Issue #5667). Appended at the end for bincode discriminant
    /// compatibility.
    MakeStepRangeLazy,
    /// Pop a collection of indices (Vector or Range), pop an array, delete the
    /// elements at those (1-based) indices, push the modified array. Implements
    /// `deleteat!(a, inds)` for `inds::AbstractVector`/`AbstractRange` (Issue #5738).
    /// Appended at the end for bincode discriminant compatibility.
    ArrayDeleteAtIndices,

    /// Fused Float64 slot square: `LoadSlotF64(slot); DupF64; MulF64`.
    /// Appended at the end for bincode discriminant compatibility.
    LoadSquareF64Slot(usize),
    /// Fused Float64 slot load plus arithmetic with the stack top:
    /// `stack <op> slot`. Appended at the end for bincode discriminant compatibility.
    LoadAddF64Slot(usize),
    LoadSubF64Slot(usize),
    LoadMulF64Slot(usize),
    LoadDivF64Slot(usize),

    /// Float64 compare + `JumpIfZero` fusion.
    ///
    /// These encode the exact false branch of the comparison result instead of
    /// the apparent opposite ordered comparison. For example, `!(a <= b)` is not
    /// equivalent to `a > b` when either side is NaN. Appended at the end for
    /// bincode discriminant compatibility.
    JumpIfEqF64(usize),
    JumpIfNeF64(usize),
    JumpIfNotLtF64(usize),
    JumpIfNotGtF64(usize),
    JumpIfNotLeF64(usize),
    JumpIfNotGeF64(usize),

    /// Add a compile-time constant to an I64 slot in place.
    ///
    /// Fuses `LoadSlotI64(slot); PushI64(k); AddI64; StoreSlotI64(slot)` and
    /// the corresponding subtraction form. Appended at the end for bincode
    /// discriminant compatibility.
    AddConstI64Slot(usize, i64),

    /// Compare two I64 slots directly and jump when `lhs > rhs`.
    ///
    /// Fuses `LoadSlotI64(lhs); LoadSlotI64(rhs); JumpIfGtI64(target)` for
    /// hot const-step loop exit tests. Appended at the end for bincode
    /// discriminant compatibility.
    JumpIfGtI64Slots(usize, usize, usize),

    /// Lazy AoT call whose arguments are read directly from I64-typed slots.
    ///
    /// Fuses `LoadSlotI64(arg)...; CallSpecialize(func, argc)` to avoid stack
    /// materialization at hot slot-argument specialized call sites. Appended at
    /// the end for bincode discriminant compatibility.
    CallSpecializeI64Slots(Box<CallSpecializeSlots>),
    /// `CallSpecializeI64Slots` entered from an `@inbounds` context.
    ///
    /// Appended at the end for bincode discriminant compatibility.
    CallSpecializeInboundsI64Slots(Box<CallSpecializeSlots>),

    /// Load a numeric slot through the `LoadSlotI64` path, convert it with the
    /// same semantics as `Float64(x)`, and push `Value::F64`.
    ///
    /// Fuses `LoadSlotI64(slot); CallBuiltin(Float64, 1)` in hot coordinate and
    /// scalar-loop code. Appended at the end for bincode discriminant
    /// compatibility.
    LoadSlotI64ToF64(usize),

    /// Add a positive constant delta to an I64 loop slot and jump to the loop
    /// body while the updated value is still `<=` the I64 stop slot.
    ///
    /// Fuses the counted-loop backedge pattern
    /// `AddConstI64Slot(slot, delta); Jump(header)` when `header` starts with
    /// `JumpIfGtI64Slots(slot, stop_slot, fallthrough_exit)`. The initial
    /// header branch remains in bytecode for empty-range semantics; this
    /// superinstruction handles only the hot backedge. Appended at the end for
    /// bincode discriminant compatibility.
    AddConstI64SlotAndJumpIfLe(usize, i64, usize, usize),

    /// Compile-time resolved direct call whose arguments are read directly from
    /// I64-typed slots.
    ///
    /// Fuses `LoadSlotI64(arg)...; CallResolved(func, argc)` to avoid stack
    /// materialization at hot direct-call sites. Appended at the end for
    /// bincode discriminant compatibility.
    CallResolvedI64Slots(Box<CallDirectSlots>),
    /// Direct `@inbounds` call whose arguments are read directly from I64-typed
    /// slots.
    ///
    /// Fuses `LoadSlotI64(arg)...; CallInbounds(func, argc)`. Appended at the
    /// end for bincode discriminant compatibility.
    CallInboundsI64Slots(Box<CallDirectSlots>),

    /// Leave a catch block that completed normally.
    ///
    /// Appended at the end for bincode discriminant compatibility.
    PopCaughtException,

    /// Load an I64 slot, add a compile-time constant, and push the result.
    ///
    /// Fuses `LoadSlotI64(slot); PushI64(delta); AddI64` for hot scalar helper
    /// bodies such as `f(x::Int64) = x + 1`. Appended at the end for bincode
    /// discriminant compatibility.
    LoadAddConstI64Slot(usize, i64),

    /// `slot[dst] = slot[lhs] + slot[rhs]` on F64 slots without touching the
    /// stack (Issue #9126).
    ///
    /// Fuses the 4-instruction F64 accumulate body
    /// `LoadSlotF64(lhs); LoadSlotF64(rhs); AddF64; StoreSlotF64(dst)`
    /// (`s = s + x` when `dst == lhs`, and the general 3-slot add otherwise)
    /// into one dispatch. Operand order: `(dst, lhs, rhs)`. Appended at the
    /// end for bincode discriminant compatibility.
    AddF64Slots(usize, usize, usize),

    /// `slot[dst] = slot[lhs] + Float64(slot[rhs_i64])` — the I64→F64
    /// converting form of [`Instr::AddF64Slots`] (Issue #9126).
    ///
    /// Fuses `LoadSlotF64(lhs); LoadSlotI64ToF64(rhs); AddF64;
    /// StoreSlotF64(dst)` — the canonical `s += Float64(i)` loop body.
    /// Operand order: `(dst, lhs, rhs_i64)`. Appended at the end for bincode
    /// discriminant compatibility.
    AddF64I64Slots(usize, usize, usize),

    /// `rand(T)` for a concrete bits-numeric / `Bool` type `T`, drawing one
    /// scalar from the VM's global RNG and pushing a `Value` of type `T`
    /// (Issue #9265). Appended at the end for bincode discriminant compatibility.
    RandScalarTyped(ScalarRandType),

    /// `rand(rng, T)` for a concrete bits-numeric / `Bool` type `T`: pop the
    /// `Rng`, push a scalar `Value` of type `T`, then push the (advanced) `Rng`
    /// back so the caller's `store_rng_back` persists the stream state
    /// (Issue #9265). Appended at the end for bincode discriminant compatibility.
    RngRandScalarTyped(ScalarRandType),

    /// `rand(rng, x)` where `x` is statically untyped (`ValueType::Any`): the
    /// stack holds the `Rng` with `x` pushed on top. Branches at runtime — a
    /// concrete scalar `DataType` value yields a scalar typed draw, a
    /// non-negative integer yields a length-`n` vector — then pushes the result
    /// and the advanced `Rng` back so the caller's `store_rng_back` persists the
    /// stream state. The explicit-RNG analogue of [`Instr::RandArg`] for the
    /// runtime-type case (Issue #9265). Appended at the end for bincode
    /// discriminant compatibility.
    RngRandArg,

    /// Filtered `Generator(map, Filter(predicate, iter))` where `map` and
    /// `predicate` are RUNTIME callable values (function/closure), not bare
    /// function indices. Pops the iterator, then the map callable, then the
    /// predicate callable (pushed in that order by the compiler), and pushes a
    /// lazy `Generator` carrying `GeneratorCallable::FilteredRuntimeValue`. The
    /// filtered analogue of [`Instr::MakeGeneratorRuntime`] for lifted
    /// body/predicate functions that live in a function scope / capture
    /// (Issue #9271). Appended at the end for bincode discriminant
    /// compatibility.
    MakeGeneratorRuntimeFiltered(Option<ArrayElementType>),
    /// `rand(a, dims...)` / `randn(a, dims...)` where `a` is statically untyped
    /// (`ValueType::Any`) so the compiler cannot distinguish an explicit `Rng`
    /// from a leading array dimension at compile time. This is the shape a
    /// captured RNG takes inside a generator/comprehension body, where it arrives
    /// as a `LoadCaptured` value of static type `Any` (Issue #9285). `argc` is the
    /// total argument count; the stack holds `a` (lowest) with the trailing args
    /// pushed above it. `is_randn` selects the standard-normal (`randn`) vs the
    /// uniform (`rand`) draw. Branches at runtime:
    ///
    /// - `a` is an `Rng` → `rand(rng, rest...)` / `randn(rng, rest...)`. For
    ///   `rand`, a leading scalar `DataType` in `rest` yields a scalar typed draw
    ///   (no trailing dims) or a typed array (with dims); otherwise every element
    ///   of `rest` is an array dimension, producing an N-dimensional `Float64`
    ///   array. The advanced `Rng` is pushed back above the result so the caller's
    ///   `store_rng_back` persists the stream state.
    /// - `a` is a dimension → `rand(dims...)` / `randn(dims...)`: every argument is
    ///   an array dimension, yielding an N-dimensional `Float64` array from the
    ///   global RNG; a placeholder is pushed above the result so the caller's
    ///   `store_rng_back` still balances.
    ///
    /// The captured-RNG analogue of [`Instr::RngRandArg`], generalized to any
    /// arity and to `randn`. Appended at the end for bincode discriminant
    /// compatibility.
    RandMaybeRng {
        argc: usize,
        is_randn: bool,
    },
    /// Discard the named hard-scope `let`-block locals from the current frame at
    /// block exit (Issue #9313). A `let ... end` is a hard local scope: a name
    /// bound (a `let` binding or a let-body assignment) inside it must NOT remain
    /// visible after the block. sjulia runs a top-level `let` body in the module
    /// frame (frame 0) with global slots, so without this the binding leaked into
    /// module scope and `@isdefined` reported it after the `let`. For each name,
    /// clear its slot (set to `None`) in the current frame and drop any named
    /// binding, matching upstream's discard-at-block-exit semantics. Names that
    /// shadow an initialized outer binding are restored separately and are NOT
    /// listed here; explicit `global` names are excluded by the compiler. The
    /// let's result value stays on the stack untouched (net zero stack effect).
    /// Appended at the end for bincode discriminant compatibility.
    ForgetLetLocals(Vec<String>),

    /// `rand(rng, T, dims...)` for a concrete bits-numeric / `Bool` / float type
    /// `T`: pop the `dims` (top of stack), pop the explicit `Rng`, push an
    /// N-dimensional array whose faithful element type is `T` (Matrix{Int64},
    /// Vector{Float32}, …) with each element drawn via the same
    /// `sample_scalar_typed` path as the scalar `rand(rng, T)` form, then push
    /// the advanced `Rng` back so the caller's `store_rng_back` persists the
    /// stream state. Replaces the element-type-dropping `RngRandArrayI64`
    /// (Float64-backed) and generalizes the leading-type array form to every
    /// scalar type sjulia accepts (Issue #9328). `Float16` has no native array
    /// storage in sjulia, so its elements are stored `Any`-backed exactly as
    /// `Float16[...]` / `zeros(Float16, n)` do. Appended at the end for bincode
    /// discriminant compatibility.
    RngRandArrayTyped(ScalarRandType, usize),

    /// `rand(T, dims...)` for a concrete bits-numeric / `Bool` / float type `T`:
    /// pop the `dims` and push a correctly-typed N-dimensional array drawn from
    /// the VM's global RNG, using the same per-element typed draw as the scalar
    /// `rand(T)` form. The global-RNG analogue of [`Instr::RngRandArrayTyped`];
    /// replaces the Float64-backed `RandIntArray` (Issue #9328). Appended at the
    /// end for bincode discriminant compatibility.
    RandArrayTyped(ScalarRandType, usize),

    /// Coerce a range's runtime `stop` bound to an `Int64` using upstream
    /// `UnitRange{Int}` / `StepRange{Int}` last-element semantics, so an
    /// integer-typed range whose bound infers `Any` but arrives as a `Float`
    /// at runtime (`[i for i in 1:n]` / `for i in 1:n` with `n = 5.5`) truncates
    /// toward the iteration direction instead of erroring with
    /// `expected I64, got Float64` (Issue #9321).
    ///
    /// Stack (top → down): `stop`, `step`, `start`. `step` and `start` are READ
    /// (peeked, NOT popped); `stop` is popped and replaced on top of stack by
    /// its integer coercion, leaving `step` and `start` untouched below it:
    /// - an integer `stop` is passed through as `I64`;
    /// - a float `stop` becomes `floor(stop)` when `step >= 0`, else `ceil(stop)`
    ///   — the last integer element reachable in the step direction, so
    ///   `length(1:5.5) == length(1:5) == 5` and `length(10:-2:2.5) == 4`
    ///   (mirrors `base/range.jl` `unitrange_last` / `steprange_last`);
    /// - a `NaN` / non-finite / out-of-`Int64`-range `stop` (float or
    ///   `BigInt`/`BigFloat`) raises the upstream `InexactError` when the range
    ///   counts toward it (`q = (stop - start) / step >= 0`, e.g. `1:Inf`,
    ///   `1:1e30`, `1:big(10)^30`), while the legal empty direction (`q < 0`,
    ///   e.g. `1:-Inf`, `10:-2:Inf`) coerces to an empty range instead — this
    ///   direction test is why `start` is part of the stack contract
    ///   (Issue #9377);
    /// - a non-numeric `stop` raises a runtime `TypeError` (same shape as
    ///   [`Instr::DynamicToI64`]).
    ///
    /// Appended at the end for bincode discriminant compatibility.
    CoerceRangeStopI64,

    /// Destructive slot load (Issue #10107): move the value out of local slot
    /// `usize`, leaving the slot empty (`None`), and push it onto the stack.
    /// Semantically identical to the verbatim-clone slot loads (`LoadSlot`,
    /// `LoadSlotStr`, `LoadSlotArray`, …) except it MOVES instead of cloning,
    /// avoiding the `Value::clone` (for reference-carrying types, an `Rc`
    /// refcount bump or a deep `Box`/`Vec` copy).
    ///
    /// SOUNDNESS: the peephole optimizer rewrites a plain `LoadSlot*(i)` to
    /// `TakeSlot(i)` ONLY when the immediately following instruction is a
    /// value-consuming `Return*` AND no exception handler is active at that
    /// point (`PushHandler`/`PopHandler` nesting is zero). Because the `Return`
    /// discards the frame right after the move, no later instruction of this
    /// frame can observe the emptied slot — the sole exception being a
    /// `finally`/`catch` handler, which the handler-nesting guard excludes.
    /// If the slot is empty on entry (undefined variable) this raises the same
    /// `UndefVarError` a plain load would. Appended at the end for bincode
    /// discriminant compatibility.
    TakeSlot(usize),

    /// Fused slot-vs-constant compare-and-branch (Issue #10105).
    ///
    /// Folds the four-instruction sequence
    /// `LoadSlotI64(slot); PushI64(konst); <cmp>I64; JumpIfZero(target)` — the
    /// canonical shape of a constant-bounded loop guard (`while i < n`) or a
    /// constant comparison in a branch — into one instruction. Reads I64 slot
    /// `slot` (with the same widening/`UndefVarError`/`TypeError` semantics as
    /// `LoadSlotI64` followed by `pop_i64`), and jumps to `target` when
    /// `cmp(slot_value, konst)` holds. `cmp` is already the branch predicate,
    /// i.e. the inverse of the source comparison (see [`I64Cmp`]): the guard
    /// `i < n` (`LtI64; JumpIfZero(exit)`, jump when `!(i < n)`) becomes
    /// `JumpIfCmpI64SlotConst(i, n, I64Cmp::Ge, exit)`.
    ///
    /// Appended at the end for bincode discriminant compatibility.
    JumpIfCmpI64SlotConst(usize, i64, I64Cmp, usize),

    /// `CallSpecialize` whose arguments are all loaded from F64 slots
    /// (Issue #10491) — the Float64 mirror of [`Instr::CallSpecializeI64Slots`].
    /// Produced by the peephole fusion of a `LoadSlotF64` run followed by
    /// `CallSpecialize` with a matching argument count. Lets the VM read the
    /// arguments directly from typed slots (skipping the stack), probe the
    /// all-`F64` specialize dispatch cache, and run the callee's predecoded
    /// frame-less `F64FunctionBlock` when it qualifies; falls back to the
    /// generic `CallSpecialize` semantics otherwise.
    /// Appended at the end for bincode discriminant compatibility.
    CallSpecializeF64Slots(Box<CallSpecializeSlots>),
    /// `CallSpecializeF64Slots` entered from an `@inbounds` context.
    /// Appended at the end for bincode discriminant compatibility.
    CallSpecializeInboundsF64Slots(Box<CallSpecializeSlots>),

    /// `Core.apply_type(args...)` with a runtime base and one or more splatted
    /// argument collections (Issues #10191, #10555).
    ///
    /// Stack layout: `[raw_arg_1, ..., raw_arg_n]`, with
    /// `n == splat_mask.len()`. Every marked argument is expanded first; the
    /// first flattened value is then the base and the remainder are its type
    /// parameters, matching Julia call-splat semantics. Appended at the end
    /// for serialized instruction compatibility.
    ApplyTypeDynamicSplat(Vec<bool>),

    /// Raise a catchable `VmError::UndefVarError` naming a global that could
    /// not be resolved to any function/method/builtin at a call site (Issue
    /// #10354's fixture-fallout measurement,
    /// `modules/module_selective_using_globals_7955.jl`; see
    /// docs/vm/EXCEPTION_PARITY.md).
    ///
    /// Unlike `ThrowError` (which pops a `String` and throws the generic
    /// `ErrorException` — the "nearest error" every other unresolved-name
    /// message used to fall back to), the undefined name is known at compile
    /// time and carried inline, mirroring `ThrowMethodError`'s shape, so the
    /// runtime raises the upstream-matching `VmError::UndefVarError` directly
    /// instead of a generic string-message `ErrorException`. Appended at the
    /// end of the enum to preserve every existing bincode discriminant.
    ThrowUndefVarError(String),

    /// Enter a root-frame lexical environment whose declared bindings start
    /// undefined. The VM owns these values separately from frame 0 so a called
    /// function continues to resolve the same-named module global (Issues
    /// #11569/#9784). Appended for serialized instruction compatibility.
    EnterLexicalScope(Vec<String>),
    /// Load the innermost lexical owner of `name`, raising `UndefVarError` when
    /// that owner is still uninitialized. Appended for wire compatibility.
    LoadLexical(String),
    /// Store the stack top into the innermost lexical owner of `name`.
    /// Appended for wire compatibility.
    StoreLexical(String),
    /// Push whether the innermost lexical owner of `name` is initialized.
    /// Appended for wire compatibility.
    IsLexicalDefined(String),
    /// Pop the innermost root-frame lexical environment and all of its values.
    /// Appended for wire compatibility.
    ExitLexicalScope,

    /// Publish a privately reserved REPL abstract type at its source position.
    /// Appended for serialized instruction compatibility (Issues #9784/#11635).
    DefineEvalAbstractType(usize),
    /// Publish a privately reserved REPL primitive type at its source position.
    /// Appended for serialized instruction compatibility (Issues #9784/#11635).
    DefineEvalPrimitiveType(usize),
    /// Runtime guard for a bare callee name in the positional/splat call
    /// path (no keyword arguments): raise a catchable `VmError::UndefVarError`
    /// naming the callee if every `FunctionInfo` registered under this name
    /// (or its runtime-normalized spelling) is currently outside the dispatch
    /// world -- a hoisted-but-not-yet-activated top-level/inline definition,
    /// e.g. one nested inside an untaken `if`/zero-iteration loop branch
    /// (Issue #11320; siblings #11286/#10461). A no-op when no `FunctionInfo`
    /// is registered under the name at all (a builtin-only/dynamically
    /// resolved callee, left to the existing call-site fallback chain).
    ///
    /// Emitted BEFORE compiling the call's positional arguments so a genuine
    /// forward reference is caught before any argument side effect runs,
    /// matching upstream's callee-before-arguments evaluation order for a
    /// keyword-free call (`Meta.lower` on `f(g()...)` resolves `Main.f`
    /// before calling `g()`). Keyword-bearing calls evaluate arguments
    /// before the callee GlobalRef upstream too (`Core.kwcall` receives the
    /// already-evaluated positional/keyword values before `Main.f` is read)
    /// and therefore never emit this guard.
    RaiseUndefVarErrorIfFunctionInvisible(String),

    /// Allocate and publish structured nominal metadata only when execution
    /// reaches this top-level control-flow statement. Appended for serialized
    /// instruction compatibility (Issue #11654).
    DefineRuntimeNominal(Box<DefineRuntimeNominalOperands>),
    /// Probe a runtime local/global binding while deliberately ignoring the
    /// compiler-owned definition registry. Signature annotations use this for
    /// a source-later type: its eagerly generated constructor function must not
    /// make the type look defined before the declaration executes. Appended for
    /// serialized compatibility (Issues #11025/#11654).
    ProbeRuntimeBinding(String),

    /// Create a closure whose exact callable candidates were resolved by the
    /// compiler. This is distinct from legacy name-only `CreateClosure` so a
    /// private lowering helper and a Julia-visible source method may safely
    /// share a spelling. Appended for bincode compatibility (Issue #9784).
    CreateResolvedClosure(Box<ResolvedClosureOperands>),
}

impl Instr {
    /// Construct a dynamic call with an explicit compiler-resolved callee
    /// identity. Keeping this constructor centralized prevents producers from
    /// dropping the semantic name while assembling bytecode (Issue #10461).
    pub fn call_dynamic(
        callee_name: impl Into<String>,
        fallback_func_index: usize,
        arg_count: usize,
        candidates: Vec<DynamicCallCandidate>,
    ) -> Self {
        Self::CallDynamic(Box::new(DynamicCallOperands {
            callee_name: callee_name.into(),
            fallback_func_index,
            arg_count,
            candidates,
        }))
    }
}

#[cfg(test)]
mod size_audit {
    use super::Instr;

    /// Guard the stride of the densely stored `Instr` enum (Issue #5176).
    ///
    /// `code: Vec<Instr>` is fetched on every dispatch cycle, so the enum's size
    /// (its largest variant, rounded up to its alignment) is paid by every
    /// instruction, including hot single-word ones like `PushI64`. Two classes
    /// of bloat are addressed:
    ///
    /// 1. Wide construction-time operand groups (`PushModule`, the
    ///    `CallTypedDispatchOrBuiltinStoreDict[Result]` 5-tuples, the
    ///    kwargs-splat invoke/call variants, `PushArrayValue`, `MakeGenerator`)
    ///    are boxed so they no longer dictate the stride.
    /// 2. `PushI128` is boxed because an inline `i128` forces 16-byte alignment
    ///    on the whole enum, padding every variant up to a multiple of 16.
    ///
    /// If a new large variant is added, prefer boxing its operands (or any
    /// 16-byte-aligned payload) rather than raising this bound; mirror the
    /// existing `Value` size audit
    /// (`value_enum.rs::test_value_enum_size_is_compact`).
    #[test]
    fn test_instr_enum_size_is_compact() {
        const MAX_INSTR_SIZE_BYTES: usize = 72;
        let size = std::mem::size_of::<Instr>();
        assert!(
            size <= MAX_INSTR_SIZE_BYTES,
            "Instr enum is {size} bytes, expected at most {MAX_INSTR_SIZE_BYTES}. \
             A new large variant likely inflated the enum's stride. Box the wide \
             operand group (String/Vec/large value payloads) behind a single \
             pointer instead of raising this bound, so the dense `code` array \
             stays I-cache friendly (Issue #5176)."
        );
    }
}
