use serde::{Deserialize, Serialize};

use super::value::{ArrayElementType, ArrayValue};
use crate::builtins::BuiltinId;
use crate::intrinsics::Intrinsic;
use half::f16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Instr {
    // constants / locals
    PushI64(i64),
    PushI128(i128),
    PushBigInt(String),
    PushBigFloat(String), // Push a BigFloat onto the stack
    PushF64(f64),
    PushF32(f32),   // Push a Float32 onto the stack
    PushF16(f16),   // Push a Float16 onto the stack
    PushBool(bool), // Push a Bool onto the stack
    PushStr(String),
    PushChar(char), // Push a Char (Unicode codepoint) onto the stack
    PushNothing,    // Push the `nothing` value onto the stack
    PushMissing,    // Push the `missing` value onto the stack
    PushUndef,      // Push the `#undef` value onto the stack (for required kwargs)
    PushStdout,     // Push the `stdout` IO value onto the stack
    PushStderr,     // Push the `stderr` IO value onto the stack
    PushStdin,      // Push the `stdin` IO value onto the stack
    PushDevnull,    // Push the `devnull` IO value onto the stack
    PushCNull,      // Push the `C_NULL` pointer value (Ptr{Cvoid}(0)) onto the stack
    PushEnv,        // Push the `ENV` dictionary (environment variables) onto the stack
    /// Push a Module value onto the stack with exports and publics
    /// Args: (name, exports, publics)
    PushModule(String, Vec<String>, Vec<String>),
    PushDataType(String), // Push a DataType value (type as a value)
    PushFunction(String), // Push a function object by name
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
    LoadSlot(usize),
    StoreSlot(usize),
    LoadAny(String),  // Dynamic load - checks all type maps at runtime
    StoreAny(String), // Dynamic store - stores based on runtime type
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
    /// Format: (fallback_func_index, arg_count, candidates)
    /// Each candidate is (func_index, expected_type_name) for single-arg functions.
    /// VM checks if arg type matches expected_type_name and calls that func_index.
    CallDynamic(usize, usize, Vec<(usize, String)>),

    /// Runtime method dispatch for binary operators when one operand has Any type.
    /// Format: (fallback_func_index, check_position, candidates)
    /// - check_position: which argument position (0 or 1) has the Any type to check
    /// - candidates: Vec<(func_index, expected_type_name)> for the checked position
    /// At runtime, VM checks the type of arg[check_position] and dispatches.
    CallDynamicBinary(usize, usize, Vec<(usize, String)>),

    /// Runtime dispatch for binary operators when both operands have Any type.
    /// Format: (fallback_intrinsic, candidates)
    /// - fallback_intrinsic: The intrinsic to use if both operands are primitives
    /// - candidates: Vec<(func_index, left_type, right_type)> for matching methods
    /// At runtime, VM checks both operand types and dispatches to matching method.
    /// If no method matches and both are primitives, uses the fallback intrinsic.
    CallDynamicBinaryBoth(Intrinsic, Vec<(usize, String, String)>),

    /// Runtime dispatch for binary operators WITHOUT builtin fallback.
    /// Used when user-defined methods shadow builtins completely.
    /// Format: candidates Vec<(func_index, left_type, right_type)>
    /// At runtime, VM matches operand types to candidates and calls matching method.
    /// Returns MethodError if no matching method is found.
    CallDynamicBinaryNoFallback(Vec<(usize, String, String)>),

    /// Runtime dispatch for unary functions with builtin fallback.
    /// Format: (builtin_id, candidates)
    /// - builtin_id: The builtin to call if argument is not a struct type
    /// - candidates: Vec<(func_index, expected_type_name)> for struct types
    /// At runtime, if the argument is a matching struct type, calls the user method.
    /// Otherwise, converts to F64 and calls the builtin.
    CallDynamicOrBuiltin(BuiltinId, Vec<(usize, String)>),

    /// Dynamic dispatch for iterate() with 1 or 2 arguments.
    /// Used when collection type is Any at compile time (e.g., parametric struct field).
    /// Format: (argc, candidates)
    /// - argc: Number of arguments (1 for initial, 2 for subsequent)
    /// - candidates: Vec<(func_index, expected_type_name)> for struct types
    /// At runtime:
    /// 1. Check if first arg (collection) is StructRef
    /// 2. If StructRef, find matching iterate method in candidates and call it
    /// 3. If not struct, use builtin iterate (IterateFirst/IterateNext)
    IterateDynamic(usize, Vec<(usize, String)>),

    /// Runtime dispatch for Type{T} patterns.
    /// Used when calling functions with ::Type{T} parameter patterns and the specific
    /// type values aren't known at compile time (e.g., promote_rule(T, S) where T,S are type params).
    /// Format: (func_name, arg_count, fallback_index, candidates)
    /// - func_name: function name for error messages
    /// - arg_count: number of arguments
    /// - fallback_index: function index to call if no specific match (generic where clause version)
    /// - candidates: Vec<(func_index, expected_type_names)> for each method
    ///   expected_type_names are the type names that TypeOf patterns expect (e.g., ["Float64", "Int64"])
    /// At runtime, pops DataType values from stack and matches against candidates.
    CallTypedDispatch(String, usize, usize, Vec<(usize, Vec<String>)>),

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

    /// Call a function variable with splatted arguments.
    /// This handles patterns like: function apply_variadic(f, args...); f(args...); end
    /// 1. Pop the Function value from stack (top of stack)
    /// 2. Pop arg_count arguments from stack
    /// 3. Expand splatted arguments based on splat_mask
    /// 4. Call the function with the expanded arguments
    /// (arg_count, splat_mask)
    CallFunctionVariableWithSplat(usize, Vec<bool>),

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
    NewArray(usize),            // Create empty array with capacity
    PushElem,                   // Pop value, push to array on stack
    FinalizeArray(Vec<usize>),  // Set shape, finalize array
    PushArrayValue(ArrayValue), // Push array value directly (for testing)

    LoadArray(String),  // Load array from local variable
    StoreArray(String), // Store array to local variable

    IndexLoad(usize),  // Pop N indices, pop array, push element
    IndexSlice(usize), // Pop N indices, pop array, push subarray
    IndexStore(usize), // Pop value, pop N indices, pop array, store, push array

    // NOTE: ArrayLen, ArraySum, ArrayShape, ArrayToSizeTuple, Zeros, Ones, Trues, Falses, Fill
    //       have been moved to CallBuiltin (Layer 2 Builtins)
    Zero, // Pop value, push zero of same type (0.0 for f64, 0+0im for complex)

    ArrayPush,      // Pop value, pop array, push to array, push array
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
    /// Store Value to typed array
    /// Pop value, pop N indices, pop array, store, push array
    IndexStoreTyped(usize),
    /// Finalize typed array with shape
    FinalizeArrayTyped(Vec<usize>),

    /// Allocate uninitialized typed array: pop `argc` dimension values, create Array{T}(undef, dims...)
    /// Handles all element types generically (Issue #2218)
    AllocUndefTyped(ArrayElementType, usize),

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
    /// findall(f, arr) - Return Int64 indices where predicate returns true
    /// Pop array, apply predicate function to each element, push Int64 array of indices
    FindAllFunc(usize), // func_index

    /// findfirst(f, arr) - Return first 1-based index where predicate returns true, or nothing
    /// Pop array, apply predicate function to each element, short-circuit on first match
    FindFirstFunc(usize), // func_index

    /// findlast(f, arr) - Return last 1-based index where predicate returns true, or nothing
    /// Pop array, apply predicate function to each element, track last match
    FindLastFunc(usize), // func_index

    /// mapreduce(f, op, arr) - Apply f to each element, then reduce with op
    /// Pop array, apply f to each element, reduce results with op, push final value
    MapReduceFunc(usize, usize), // (map_func_index, reduce_func_index)

    /// mapreduce(f, op, arr, init) - Apply f to each element, then reduce with op and init
    /// Pop init, pop array, apply f, reduce with op starting from init, push final value
    MapReduceFuncWithInit(usize, usize), // (map_func_index, reduce_func_index)

    /// mapfoldr(f, op, arr) - Apply f to each element, then right-fold with op
    /// Pop array, apply f to each element in reverse, reduce results with op (swapped args), push final value
    MapFoldrFunc(usize, usize), // (map_func_index, reduce_func_index)

    /// mapfoldr(f, op, arr, init) - Apply f to each element, then right-fold with op and init
    /// Pop init, pop array, apply f in reverse, reduce with op (swapped args) starting from init, push final value
    MapFoldrFuncWithInit(usize, usize), // (map_func_index, reduce_func_index)

    /// map!(f, dest, src) - Apply f to each element of src, store in dest
    /// Pop src array, pop dest array, apply f, store results in dest, push dest
    MapFuncInPlace(usize), // func_index

    /// filter!(f, arr) - Remove elements where predicate returns false
    /// Pop array, apply predicate, remove non-matching elements, push array
    FilterFuncInPlace(usize), // func_index

    // Note: ForEachFunc removed - foreach is now Pure Julia (base/abstractarray.jl)
    /// sum(f, arr) - Apply function to each element and sum the results
    /// Pop array, apply function to each element, push sum
    SumFunc(usize), // func_index

    /// any(f, arr) - Check if predicate returns true for any element
    /// Pop array, apply predicate to each element, push bool (1 or 0)
    AnyFunc(usize), // func_index

    /// all(f, arr) - Check if predicate returns true for all elements
    /// Pop array, apply predicate to each element, push bool (1 or 0)
    AllFunc(usize), // func_index

    /// count(f, arr) - Count elements where predicate returns true
    /// Pop array, apply predicate to each element, push count
    CountFunc(usize), // func_index

    /// ntuple(f, n) - Create tuple by calling f(i) for i in 1:n
    NtupleFunc(usize), // func_index

    /// Generator(f, iter) - Create lazy generator
    /// Pop iterator, push Generator with function index
    MakeGenerator(usize), // func_index

    /// Wrap an array in a Generator for eager-evaluated generator expressions
    /// Pop Array, push Generator wrapping that array
    WrapInGenerator,

    /// sprint(f, args...) - Call f(io, args...) and return the result as a string
    /// Pop N args, create IOBuffer, call function with IOBuffer + args, push string
    SprintFunc(usize, usize), // (func_index, arg_count)

    // RNG instance operations
    NewStableRng,            // pop seed(I64), push Rng(StableRng)
    NewXoshiro,              // pop seed(I64), push Rng(Xoshiro)
    LoadRng(String),         // load RNG from local variable
    StoreRng(String),        // store RNG to local variable
    RngRandF64,              // pop Rng, push F64, push Rng (mutated)
    RngRandArrayF64(usize),  // pop dims, pop Rng, push Array[F64], push Rng
    RngRandArrayI64(usize),  // pop dims, pop Rng, push Array[I64], push Rng
    RngRandnF64,             // pop Rng, push randn() F64, push Rng
    RngRandnArrayF64(usize), // pop dims, pop Rng, push randn Array[F64], push Rng
    ReturnRng,               // Return RNG value

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

    // Dict operations
    NewDict,                 // Create empty dict, push
    NewDictWithPairs(usize), // Pop N pairs (key, value), create dict, push
    NewDictTyped(String, String), // Create empty dict with (K, V) type params, push
    LoadDict(String),        // Load dict from local
    StoreDict(String),       // Store dict to local
    DictSet,                 // Pop value, pop key, pop dict, set, push dict
    DictLen,                 // Pop dict, push length as I64
    // NOTE: DictGet, DictGetDefault, DictHasKey, DictDelete, DictKeys,
    //       DictValues, DictPairs, DictMerge moved to CallBuiltin (Layer 2)
    ReturnDict, // Return dict value

    // Set operations
    NewSet,           // Create empty set, push
    SetAdd,           // Pop element, pop set, add element, push set
    StoreSet(String), // Store set to local
    LoadSet(String),  // Load set from local
    ReturnSet,        // Return set value

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

    // Memory{T} operations
    /// Create a new Memory{T} with given element type and length (undef-initialized).
    /// Pushes Memory value onto stack.
    NewMemory(ArrayElementType, usize),
    /// Create a new Memory{T} with given element type and dynamic length from stack.
    /// Pops I64 length from stack, pushes Memory value onto stack.
    NewMemoryDynamic(ArrayElementType),
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

    /// No-operation placeholder used during instruction dispatch (Issue #2939).
    /// Never appears in compiled bytecode; only used transiently in the execution loop
    /// to avoid cloning instructions on every cycle.
    Nop,
}
