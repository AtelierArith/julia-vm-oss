//! Core Builtins - Layer 2 of the Julia three-layer architecture.
//!
//! This module defines built-in functions that are implemented in Rust
//! but are not CPU-level operations (those are in `intrinsics.rs`).
//!
//! # Architecture
//!
//! ```text
//! Layer 3: Julia Code (prelude/*.jl)
//! Layer 2: Builtins (this module) <- map, filter, round, floor, ...
//! Layer 1: Intrinsics (intrinsics.rs) <- add_int, mul_float, ...
//! ```
//!
//! # Design Principle
//!
//! - Intrinsics: CPU instructions (add, mul, compare, sqrt, floor, ceil)
//! - Builtins: Library functions (map, filter, print, round, floor, ceil)

use serde::{Deserialize, Serialize};

/// Built-in function identifiers.
///
/// These functions are implemented in Rust and called via `CallBuiltin` instruction.
/// Unlike Intrinsics (CPU-level operations), Builtins are higher-level library functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuiltinId {
    // =========================================================================
    // Math Functions
    // =========================================================================

    // Note: Sin, Cos, Tan, Asin, Acos, Atan removed — now Pure Julia (base/math.jl)
    // Note: Exp, Log removed — now Pure Julia (base/math.jl)
    Sqrt, // sqrt(x) fallback after method dispatch

    // Rounding (these ARE CPU instructions but we keep them as builtins for consistency)
    Floor,          // floor(x) - round down to nearest integer
    FloorDigits,    // floor(x, digits=N) - floor to N decimal places (Issue #2054)
    FloorSigDigits, // floor(x, sigdigits=N) - floor to N significant digits (Issue #2054)
    Ceil,           // ceil(x) - round up to nearest integer
    CeilDigits,     // ceil(x, digits=N) - ceil to N decimal places (Issue #2054)
    CeilSigDigits,  // ceil(x, sigdigits=N) - ceil to N significant digits (Issue #2054)
    Round,
    RoundDigits,    // round(x, digits=N) - round to N decimal places (Issue #2051)
    RoundSigDigits, // round(x, sigdigits=N) - round to N significant digits (Issue #2051)
    Trunc,
    TruncDigits,    // trunc(x, digits=N) - trunc to N decimal places (Issue #2059)
    TruncSigDigits, // trunc(x, sigdigits=N) - trunc to N significant digits (Issue #2059)

    // Float adjacency (IEEE 754 bit manipulation)
    // NextFloat removed - pure Julia (base/float.jl, Issue #6740)
    // PrevFloat removed - pure Julia (base/float.jl, Issue #6740)

    // Bit operations: low-level CPU intrinsics. The public functions count_ones /
    // leading_zeros / trailing_zeros / bswap / bitreverse are pure Julia
    // (base/int.jl) and call these via underscored names _ctpop_int / _ctlz_int /
    // _cttz_int / _bswap_int / _bitreverse_int (Issue #6741). The derived helpers
    // count_zeros / leading_ones / trailing_ones / bitrotate are pure Julia too
    // (Issue #6722). These variants keep their identifiers but expose the
    // underscored intrinsic names through from_name()/name().
    CountOnes,     // _ctpop_int  - popcount (number of 1 bits)
    LeadingZeros,  // _ctlz_int   - leading zero bits
    TrailingZeros, // _cttz_int   - trailing zero bits
    Bitreverse,    // _bitreverse_int - reverse all bits
    Bswap,         // _bswap_int  - byte swap (reverse byte order)

    // Float decomposition (IEEE 754)
    // Exponent removed - pure Julia (base/float.jl, Issue #6740)
    // Significand removed - pure Julia (base/float.jl, Issue #6740)
    // Frexp removed - pure Julia (base/float.jl, Issue #6740)

    // Float inspection
    // Issubnormal removed - pure Julia (base/float.jl, Issue #6740)
    // Note: Maxintfloat removed — Pure Julia (base/floatfuncs.jl, Issue #3732).

    // Fused multiply-add (internal _fma intrinsic only)
    // Public fma/muladd are Pure Julia (base/math.jl). The Pure Julia wrapper
    // for Float64 calls `_fma` to preserve IEEE fused semantics; muladd is
    // expressed as plain `x*y + z`.
    Fma, // _fma(x::Float64, y::Float64, z::Float64) = x*y + z (fused, single rounding)

    // Note: Abs is now Pure Julia (number.jl, int.jl, float.jl, bool.jl, complex.jl)

    // Unary negation with runtime dispatch for struct types
    NegAny,

    // Number theory - now Pure Julia (base/intfuncs.jl)
    // Gcd, Lcm, Factorial removed - use Pure Julia implementations

    // =========================================================================
    // Array Operations
    // =========================================================================

    // Creation
    Zeros,
    ZerosF64, // zeros(Float64, dims...) - create Float64 array
    ZerosI64, // zeros(Int64, dims...) - create Int64 array
    // Note: ZerosComplexF64 removed (Issue #5156). `zeros(Complex{Float64}, ...)`
    // now routes through pure-Julia generic `zeros`/`_array_undef_from_dims`
    // dispatch + the generic typed-allocation path (interleaved storage).
    Ones,
    OnesF64, // ones(Float64, dims...) - create Float64 array
    OnesI64, // ones(Int64, dims...) - create Int64 array
    // Note: Trues, Falses, Fill are now Pure Julia (base/array.jl) — Issue #2640
    Similar,
    // Uninitialized array allocation: Vector{T}(undef, n), Array{T}(undef, dims...)
    AllocUndefF64, // Array{Float64}(undef, dims...) - create uninitialized Float64 array
    AllocUndefI64, // Array{Int64}(undef, dims...) - create uninitialized Int64 array
    // Note: AllocUndefComplexF64 removed (Issue #5156).
    // `Array{Complex{Float64}}(undef, ...)` routes through the generic
    // typed-allocation path (interleaved storage + Complex struct type_id).
    AllocUndefBool, // Array{Bool}(undef, dims...) - create uninitialized Bool array
    AllocUndefAny,  // Array{Any}(undef, dims...) - create uninitialized Any array
    MarkBitVector,  // _mark_bitvector(v) - retag Bool vector as BitVector (Issue #5484)
    MarkBitArray, // _mark_bitarray(v) - retag Bool array as BitVector/BitMatrix/BitArray{N} (Issue #5498)
    // Copy: Now implemented in Pure Julia (base/array.jl)
    Reshape,

    // Query
    Length,
    Size,
    Ndims,
    Eltype,
    Keytype,
    Valtype,
    MemoryRefNew,    // memoryref/memoryrefnew - create MemoryRef{T}
    MemoryRefGet,    // memoryrefget(ref, order, boundscheck)
    MemoryRefSet,    // memoryrefset!(ref, value, order, boundscheck)
    MemoryRefOffset, // memoryrefoffset(ref) - 1-based parent offset
    MemoryRefParent, // memoryrefparent(ref) - parent Memory{T}

    // Manipulation
    Push,      // push!
    Pop,       // pop!
    PushFirst, // pushfirst!
    PopFirst,  // popfirst!
    Insert,    // insert!
    DeleteAt,  // deleteat!
    Append,    // append!
    Prepend,
    // Reverse: Now implemented in Pure Julia (base/array.jl, base/sort.jl)
    // Sort: Now Pure Julia (base/sort.jl) — Issue #3725

    // Aggregation
    // Sum, Prod, Minimum, Maximum: Now Pure Julia (base/array.jl). The dead
    // Prod/Minimum/Maximum BuiltinId variants were removed (Issue #6745).
    // Mean: Now Pure Julia (stdlib/Statistics/src/Statistics.jl)

    // Statistics: All now Pure Julia (stdlib/Statistics/src/Statistics.jl)
    // Var, Varm, Std, Stdm, Median, Middle, Cov, Cor, Quantile

    // Search: argmin/argmax/findfirst/findall are Pure Julia (base/array.jl);
    // their dead BuiltinId variants were removed (Issue #6745).

    // =========================================================================
    // Higher-Order Functions
    // =========================================================================
    // Note: map, filter, reduce, foldl, foldr, foreach, ntuple are now Pure Julia
    Any,
    All,
    Count,
    Compose, // compose(f, g) - create composed function f ∘ g

    // =========================================================================
    // Range Operations
    // =========================================================================
    RangeNew, // range(start, stop, step)
    RangeCollect,
    LinRange, // range(start, stop, length=n)

    // =========================================================================
    // Complex Number Operations
    // =========================================================================
    // Note: the back-compat `Complex` builtin id (no VM handler, not reachable
    // via `from_name`) was removed (Issue #5156). Public `complex(...)` is Pure
    // Julia (base/complex.jl, Issue #3727); real/imag/conj/angle are Pure Julia
    // (Issue #2640).

    // =========================================================================
    // String Operations
    // =========================================================================
    StringNew,       // string(...)
    StringFromChars, // String(::Vector{Char}) - char array to string (Issue #2038)
    Repr,            // repr(x)
    Sprintf,         // sprintf(fmt, args...) - formatted string
    PrintfFmtFloat, // _printf_fmt_float(x, conv::Char, prec::Int) - C float→string boundary (Issue #6746)

    // String query methods
    Ncodeunits, // ncodeunits(s) - number of bytes
    Codeunit,   // codeunit(s, i) - get byte at position i
    CodeUnits,  // codeunits(s) - get all bytes as Vector{UInt8}

    // String character access
    // StringFirst removed - now Pure Julia in base/strings/basic.jl
    // StringLast removed - now Pure Julia in base/strings/basic.jl

    // String case conversion
    // Uppercase, Lowercase, Titlecase removed - now Pure Julia (base/strings/unicode.jl)

    // String trimming - now Pure Julia (base/strings/util.jl)
    // Strip, Lstrip, Rstrip, Chomp, Chop removed - use Pure Julia implementations

    // String search/check
    // Note: startswith, endswith, join, replace are now Pure Julia (base/strings/)
    // Note: occursin(String, String) is now Pure Julia (base/strings/search.jl)
    //       occursin(Regex, String) still uses RegexOccursin builtin
    Occursin, // occursin(needle, haystack) - kept for Regex support
    // Findfirst, Findlast removed - now Pure Julia (base/strings/search.jl)

    // String manipulation
    // StringSplit removed - now Pure Julia in base/strings/util.jl
    // StringRsplit removed - now Pure Julia in base/strings/util.jl
    // StringRepeat removed - now Pure Julia in base/strings/basic.jl
    // StringReverse removed - now Pure Julia in base/strings/basic.jl

    // String conversion
    // StringToInt removed - now Pure Julia (base/parse.jl)
    // StringToFloat removed - parse(Float64,s) is Pure Julia (base/parse.jl):
    // tryparse + ArgumentError, over the _tryparse_float64 intrinsic (Issue #6748)
    // StringToIntBase removed - now Pure Julia (base/parse.jl `_parse_int_base`,
    // Issue #7875): parse(Int, s; base=N) is rewritten by the compiler to a
    // positional pure-Julia call wrapping `_tryparse_int`.
    StringIntToBase, // string(x; base=N) (Issue #2036)
    CharToInt,       // Int(c) - char to codepoint
    // Codepoint removed - pure Julia (Issue #6747)
    IntToChar, // Char(n) - codepoint to char
    // Bitstring removed - pure Julia (Issue #6747)
    // Ascii removed - now Pure Julia in base/strings/util.jl
    // Nextind, Prevind, Thisind, Reverseind removed - now Pure Julia (base/strings/basic.jl)
    // Bytes2Hex, Hex2Bytes removed - now Pure Julia (base/strings/util.jl)
    // UnescapeString removed - now Pure Julia (base/strings/util.jl, Issue #6724)
    // Isnumeric removed - now Pure Julia (base/strings/unicode.jl, Issue #6752):
    // isnumeric(c::Char) binary-searches an embedded Nd/Nl/No codepoint range
    // table generated from upstream utf8proc.
    /// Retag a `Vector{String}` as `Vector{SubString{String}}` for display
    /// purposes (Issue #3574). Used by `split`/`rsplit` so their results
    /// `show` as `SubString{String}["a", "b"]` like Julia 1.12. The underlying
    /// values stay `Value::Str` — only the array's `element_type_override`
    /// changes. Internal helper, not part of Julia's public API.
    SubStringRetag,
    IsvalidIndex, // isvalid(s, i) - check if index is valid character boundary
    // FindNextString, FindPrevString removed - now Pure Julia (base/strings/search.jl)
    // TryparseInt64 removed - now Pure Julia (base/parse.jl)
    // _tryparse_float64 intrinsic (libc strtod): the public tryparse/parse
    // (Float64,s) wrappers are Pure Julia (base/parse.jl, Issue #6748)
    TryparseFloat64, // _tryparse_float64(s) -> Float64 or nothing
    // StringCount / StringFindAll removed - dead (count/findall on String/Char
    // patterns are pure Julia in base/strings/search.jl, Issue #6724)

    // =========================================================================
    // I/O Operations
    // =========================================================================
    Print,
    Println,
    IOBufferNew, // IOBuffer() - create new IOBuffer
    TakeString,  // take!(io) or takestring!(io) - extract string from IOBuffer
    IOWrite,     // write(io, x) - write to IOBuffer
    IOPrint,     // print(io, args...) - print multiple args to IOBuffer, returns IO
    Displaysize, // displaysize() - return terminal size as (rows, cols)

    // Source file loading (no-ops, not needed for static compilation)
    IncludeDependency, // include_dependency(path) - track file dependency (no-op)
    Precompile,        // __precompile__(flag) - control precompilation (no-op)

    // Path/Filesystem Operations
    // Note: dirname, basename, joinpath, splitext, splitdir, isabspath, isdirpath
    // are now Pure Julia (base/path.jl) — Issue #2637
    Normpath, // normpath(path) - normalize path
    Abspath,  // abspath(path) - convert to absolute path
    Homedir,  // homedir() - get home directory

    // File I/O Operations (read-only; write support tracked in Issue #454)
    ReadFile,   // read(filename, String) - read entire file as String
    ReadLines,  // readlines(filename) - read all lines as Vector{String}
    Eachline,   // eachline(filename) - iterable lines for file initialization
    Readline,   // readline(filename) - read first line from file
    Countlines, // countlines(filename) - count lines in file
    Isfile,     // isfile(path) - check if path is a file
    Isdir,      // isdir(path) - check if path is a directory
    Ispath,     // ispath(path) - check if path exists
    Filesize,   // filesize(path) - get file size in bytes
    Pwd,        // pwd() - get current working directory
    Readdir,    // readdir(path) - list directory contents
    Mkdir,      // mkdir(path) - create directory
    Mkpath,     // mkpath(path) - create directory and parents
    Rm,         // rm(path) - remove file or empty directory
    Tempdir,    // tempdir() - get system temp directory
    Tempname,   // tempname() - generate unique temp filename
    Touch,      // touch(path) - create empty file or update mtime
    Cd,         // cd(path) - change directory
    Islink,     // islink(path) - check if path is a symbolic link
    Cp,         // cp(src, dst) - copy file
    Mv,         // mv(src, dst) - move/rename file
    Mtime,      // mtime(path) - get modification time as Unix timestamp

    // File Handle Operations
    Open,       // open(filename) or open(filename, mode) - open file
    Close,      // close(io) - close file
    Eof,        // eof(io) - check if at end of file
    Isopen,     // isopen(io) - check if IO stream is open
    ReadlineIo, // readline(io) - read line from IO stream

    // =========================================================================
    // Random Number Generation
    // =========================================================================
    Rand,    // rand() or rand(dims...)
    Randn,   // randn() or randn(dims...)
    RandInt, // rand(Int, dims...)

    // =========================================================================
    // Time Operations
    // =========================================================================
    TimeNs, // time_ns()
    Sleep,  // sleep(seconds)

    // =========================================================================
    // Type Operations
    // =========================================================================
    TypeOf,   // typeof(x)
    TypeVar,  // TypeVar(name[, lb, ub]) - fresh runtime TypeVar object
    UnionAll, // UnionAll(var::TypeVar, body) - wrap body in a UnionAll (Issue #4694)
    Isa,      // isa(x, T)
    Sizeof,   // sizeof(x) - size in bytes
    // Isbits removed - pure Julia (Issue #6738)
    Isbitstype,    // isbitstype(T) - is T a bits type
    _Supertype,    // _supertype(T) - get parent type (internal, Issue #3762)
    _Typename,     // _typename(T) - canonical TypeName symbol (internal, Issue #5106)
    _FunctionName, // _function_name(f) - function name symbol (internal, Issue #5580)
    Subtypes,      // subtypes(T) - vector of direct subtypes
    // Typeintersect and Typejoin removed - now Pure Julia (base/reflection.jl)
    // Fieldcount removed - now Pure Julia (base/reflection.jl)
    // Hasfield removed - pure Julia (Issue #6738)
    // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
    // removed - now Pure Julia (base/reflection.jl) with internal type-flag intrinsics
    // Ismutable removed - pure Julia (Issue #6738)
    // NameOf removed - now Pure Julia (base/reflection.jl)

    // =========================================================================
    // Object Identity / Equality
    // =========================================================================
    Egal,    // === (object identity)
    NotEgal, // !== (object non-identity)
    Isequal, // isequal(x, y) - NaN-aware equality
    // Compiler-internal: `==` folded over Tuple/NamedTuple elements (Issue
    // #5267). Unlike `Isequal` this uses `==` element semantics, so
    // `(0.0,) == (-0.0,)` is true and `(NaN,) == (NaN,)` is false. Emitted by
    // the early tuple route in `compile/expr/binary/mod.rs`; not user-callable.
    TupleEquals,
    Isless,      // isless(x, y) - strict weak ordering for sorting
    Hash,        // hash(x) - compute hash value
    Objectid,    // objectid(x) - unique object identifier
    Isunordered, // isunordered(x) - check if x is unordered (NaN, Missing)
    Subtype,     // <: (subtype check)
    SupertypeOp, // >: (supertype check - A >: B means B <: A)

    // =========================================================================
    // Set Operations
    // =========================================================================
    In, // in(x, collection) - check if element is in collection (∈ operator)

    // =========================================================================
    // Type Conversion
    // =========================================================================
    Convert,   // convert(T, x) - convert x to type T
    Promote,   // promote(x, y, ...) - promote to common type
    Signed,    // signed(x) - convert to signed integer (same bit width)
    Unsigned,  // unsigned(x) - convert to unsigned integer (same bit width)
    FloatConv, // float(x) - dead: float is Pure Julia (base/number.jl); kept
    // only because the (unreachable) handler is large. (Issue #3727/#6737)
    // Widemul removed - Pure Julia (base/number.jl widen(x)*widen(y), Issue #6737)
    Reinterpret, // reinterpret(T, x) - bit-level type reinterpretation

    // =========================================================================
    // Copy Operations
    // =========================================================================
    Deepcopy, // deepcopy(x) - recursive deep copy

    // =========================================================================
    // Reflection / Introspection (Internal VM builtins)
    // =========================================================================
    _Fieldnames,        // _fieldnames(T) - tuple of field names (internal)
    _Fieldtypes,        // _fieldtypes(T) - tuple of field types (internal)
    _Fieldoffset,       // _fieldoffset(T, i) - byte offset of a field (internal)
    _DatatypeAlignment, // _datatype_alignment(T) - byte alignment of a type (internal, Issue #5107)
    _Allocatedinline,   // _allocatedinline(T) - whether T is stored inline (internal, Issue #5107)
    _Getfield,          // _getfield(x, i) - get field value by index (internal)
    _Isabstracttype,    // _isabstracttype(T) - check abstract type (internal)
    _Isconcretetype,    // _isconcretetype(T) - check concrete type (internal)
    _Ismutabletype,     // _ismutabletype(T) - check mutable type (internal)
    _Isprimitivetype,   // _isprimitivetype(T) - check primitive type (internal, Issue #3767)
    _Isstructtype,      // _isstructtype(T) - check struct type (internal)
    _Typeintersect,     // _typeintersect(a, b) - type intersection (internal)
    _TypeUnion,         // _type_union(types...) - construct a small Union type (internal)
    _MakeTupleType, // _make_tuple_type(types) - construct Tuple{types...} from a collection (internal, Issue #5119)
    _TypeParameters, // _type_parameters(T) - tuple of type parameters (internal, Issue #3770)
    _Hash,          // _hash(x) - compute hash value (internal, Issue #2582)
    _Eltype,        // _eltype(x) - get element type (internal, Issue #2570)
    // _Dict* carrier intrinsics removed with `Value::Dict` (Issue #6731).
    Getfield,        // getfield(x, name) or getfield(x, i) - get field by name or index
    Setfield,        // setfield!(x, name, v) or setfield!(x, i, v) - set field by name or index
    _MethodsByFtype, // _methods_by_ftype(f[, types]) - method query intrinsic (Issue #3772)
    HasMethod,       // hasmethod(f, types) - check if method exists
    Which,           // which(f, types) - get specific method
    IsExported,      // isexported(m::Module, s::Symbol) - check if symbol is exported
    IsPublic,        // ispublic(m::Module, s::Symbol) - check if symbol is public (Julia 1.11+)
    IsdefinedModuleBinding, // _isdefined_module_binding(m::Module, s::Symbol) - check module binding (Issue #5002/#4958)

    // =========================================================================
    // Tuple/Dict Operations
    // =========================================================================
    TupleNew,
    TupleFirst,
    TupleLast,
    TupleLen,

    // Dict ops dispatch to the pure-Julia `Dict{K,V}` methods. These BuiltinIds
    // are thin struct-dispatch trampolines (no `Value::Dict` carrier, Issue
    // #6731). DictNew/DictMerge/DictLen had no emit sites and were removed.
    DictGet,       // get(dict, key) or get(dict, key, default)
    DictGetkey,    // getkey(dict, key, default) - return key if exists, else default
    DictSet,       // setindex!(dict, value, key) or dict[key] = value
    DictDelete,    // delete!(dict, key)
    DictHasKey,    // haskey(dict, key)
    DictKeys,      // keys(dict)
    DictValues,    // values(dict)
    DictPairs,     // pairs(dict)
    DictGetBang,   // get!(dict, key, default) - get or insert default
    DictMergeBang, // merge!(dict1, dict2) - merge in-place
    DictEmpty,     // empty!(dict) - remove all entries
    DictPop,       // pop!(dict, key) or pop!(dict, key, default) - remove and return value

    // =========================================================================
    // Set Operations
    // =========================================================================
    // Note: SetUnion, SetIntersect, SetSetdiff, SetSymdiff, SetIssubset,
    // SetIsdisjoint, SetIssetequal, and the mutating variants
    // (SetUnionMut, SetIntersectMut, SetSetdiffMut, SetSymdiffMut) are now
    // Pure Julia (subset_julia_vm/src/julia/base/set.jl) — Issue #3724.

    // =========================================================================
    // Matrix Operations
    // =========================================================================
    // Note: Transpose and Adjoint have been migrated to Pure Julia
    // See: subset_julia_vm/src/julia/base/array.jl (transpose, adjoint for arrays)
    //      subset_julia_vm/src/julia/base/number.jl (transpose, adjoint for scalars)
    //      subset_julia_vm/src/julia/base/complex.jl (transpose, adjoint for Complex)

    // Linear algebra operations (via faer library)
    Lu,       // lu(A) - LU decomposition with partial pivoting
    Det,      // det(A) - matrix determinant
    Inv,      // inv(A) - matrix inverse (only for Array types, not Rational)
    Ldiv,     // A \ b - left division (solve Ax = b for x)
    Svd,      // svd(A) - singular value decomposition (returns named tuple with U, S, V, Vt)
    Qr,       // qr(A) - QR decomposition (returns named tuple with Q, R)
    Eigen,    // eigen(A) - eigenvalues/eigenvectors (returns named tuple)
    Eigvals,  // eigvals(A) - eigenvalues of matrix (returns complex array)
    Cholesky, // cholesky(A) - Cholesky decomposition (returns named tuple with L, U)
    Rank,     // rank(A) - matrix rank (number of non-zero singular values)
    Cond,     // cond(A) - matrix condition number (2-norm)

    // =========================================================================
    // Broadcast Control
    // =========================================================================
    RefNew,    // Ref(x) - protect from broadcasting
    RefUnwrap, // x[] - unwrap Ref

    // =========================================================================
    // Zero/One constructors
    // =========================================================================
    Zero, // zero(x) or zero(T)
    One,  // one(x) or one(T)

    // =========================================================================
    // Numeric Type Constructors
    // =========================================================================
    // Signed integers
    Int8,   // Int8(x) - convert to Int8
    Int16,  // Int16(x) - convert to Int16
    Int32,  // Int32(x) - convert to Int32
    Int64,  // Int64(x) - convert to Int64
    Int128, // Int128(x) - convert to Int128
    // Unsigned integers
    UInt8,   // UInt8(x) - convert to UInt8
    UInt16,  // UInt16(x) - convert to UInt16
    UInt32,  // UInt32(x) - convert to UInt32
    UInt64,  // UInt64(x) - convert to UInt64
    UInt128, // UInt128(x) - convert to UInt128
    // Floating point
    Float16, // Float16(x) - convert to Float16
    Float32, // Float32(x) - convert to Float32
    Float64, // Float64(x) - convert to Float64

    // =========================================================================
    // BigInt Operations
    // =========================================================================
    BigInt, // BigInt(x) - convert to arbitrary precision integer

    // =========================================================================
    // BigFloat Operations
    // =========================================================================
    BigFloat,                    // BigFloat(x) - convert to arbitrary precision float
    BigFloatPrecision,           // _bigfloat_precision(x) - get precision of a BigFloat value
    BigFloatDefaultPrecision,    // _bigfloat_default_precision() - get default precision
    SetBigFloatDefaultPrecision, // _set_bigfloat_default_precision!(n) - set default precision
    BigFloatRounding,            // _bigfloat_rounding() - get current rounding mode
    SetBigFloatRounding,         // _set_bigfloat_rounding!(mode) - set rounding mode

    // =========================================================================
    // Subnormal (Denormal) Float Control
    // =========================================================================
    GetZeroSubnormals, // get_zero_subnormals() - check if subnormals are flushed to zero
    SetZeroSubnormals, // set_zero_subnormals(yes) - enable/disable flushing subnormals to zero

    // =========================================================================
    // Missing Value Utilities
    // =========================================================================
    NonMissingType, // nonmissingtype(T) - remove Missing from Union type

    // =========================================================================
    // Iterator Protocol (Julia-compatible)
    // =========================================================================
    Iterate, // iterate(collection) or iterate(collection, state)
    // Note: Collect functionality is handled by RangeCollect (which works for all iterables)

    // =========================================================================
    // Macro System (Metaprogramming)
    // =========================================================================
    SymbolNew,             // Symbol("name") - create a Symbol from string
    ExprNew,               // Expr(head, args...) - create an Expr AST node
    ExprNewWithSplat,      // Expr(head, args...) with splat expansion at runtime
    Gensym,                // gensym() or gensym("base") - generate unique symbol for hygiene
    Esc,                   // esc(expr) - escape expression for macro hygiene
    QuoteNodeNew,          // QuoteNode(value) - wrap value in QuoteNode
    LineNumberNodeNew,     // LineNumberNode(line) or LineNumberNode(line, file)
    GlobalRefNew,          // GlobalRef(mod, name) - create a global reference
    Eval,                  // eval(expr) - evaluate an Expr AST at runtime
    MetaParse,             // _meta_parse(str) - parse string to Expr (Meta.parse)
    MetaParseAt,           // _meta_parse_at(str, pos) - parse at position, return (expr, next_pos)
    MetaIsExpr,            // Meta.isexpr(ex, head) or Meta.isexpr(ex, head, n)
    MetaQuot,              // Meta.quot(ex) - wrap expression in :quote Expr
    MetaIsIdentifier,      // Meta.isidentifier(s) - check if valid identifier
    MetaIsOperator,        // Meta.isoperator(s) - check if operator symbol
    MetaIsUnaryOperator,   // Meta.isunaryoperator(s) - check if unary operator
    MetaIsBinaryOperator,  // Meta.isbinaryoperator(s) - check if binary operator
    MetaIsPostfixOperator, // Meta.ispostfixoperator(s) - check if postfix operator
    MetaLower,             // _meta_lower(expr) - lower expression to Core IR
    MacroExpand,           // macroexpand(m, x) - return expanded form of macro call
    MacroExpandBang, // macroexpand!(m, x) - destructively expand macro call (same behavior in SubsetJuliaVM)
    IncludeString,   // include_string(m, code) - parse and evaluate code string
    EvalFile,        // evalfile(path) - evaluate all expressions in a file

    // =========================================================================
    // Test Operations (for Pure Julia @test/@testset/@test_throws macros)
    // =========================================================================
    TestRecord,       // _test_record!(passed, msg) - record test result
    TestRecordBroken, // _test_record_broken!(passed, msg) - record broken test result
    TestSetBegin,     // _testset_begin!(name) - begin test set
    TestSetEnd,       // _testset_end!() - end test set and print summary

    // =========================================================================
    // Regex Operations
    // =========================================================================
    RegexNew,       // Regex(pattern) or Regex(pattern, flags) - create regex
    RegexMatch,     // match(regex, string) - find first match, returns RegexMatch or nothing
    RegexOccursin,  // occursin(regex, string) - check if regex matches anywhere in string
    RegexReplace,   // replace(string, regex => replacement) - replace matches
    RegexSplit,     // split(string, regex) - split string by regex
    RegexEachmatch, // eachmatch(regex, string) - return iterator of all matches (collected as Vector)

    // Appended for bincode compatibility with existing precompiled Base caches.
    _UnionAllVar,  // _unionall_var(T) - bound TypeVar for UnionAll-like types (internal)
    _UnionAllBody, // _unionall_body(T) - body type for UnionAll-like types (internal)
    _TypeVarName,  // _type_var_name(T) - TypeVar name as Symbol (internal)
    _TypeVarLowerBound, // _type_var_lower_bound(T) - TypeVar lower bound (internal)
    _TypeVarUpperBound, // _type_var_upper_bound(T) - TypeVar upper bound (internal)
    // Appended after the above for bincode discriminant compatibility (Issue #5676).
    EndsWithRegex, // _endswith_regex(string, regex) - true iff regex matches ending at end of string
    // IOBuffer(s::AbstractString) — a readable buffer initialized with `s` (Issue
    // #5686). Appended at the end for bincode discriminant compatibility.
    IOBufferFromString,
    // NextFloatN removed - pure Julia (base/float.jl, Issue #6740)
    // PrevFloatN removed - pure Julia (base/float.jl, Issue #6740)
    // _compose_exception_type(f, types) — interprocedural exception type composed
    // from a user function body's callees (Issue #5600). Appended at the end for
    // bincode discriminant compatibility.
    ComposeExceptionType,
    // _return_types_by_ftype(f, types) — return-type reflection through the
    // dispatch resolver, preserving no-method / ambiguous-call emptiness (Issue
    // #5603). Appended at the end for bincode discriminant compatibility.
    _ReturnTypesByFtype,
    // Compiler-internal `@generated` fallback eval that records the returned
    // staged Expr under the concrete argument tuple key (Issue #5936).
    // Appended for bincode discriminant compatibility.
    GeneratedEval,
    // Native-indexing fallback for `getindex` when a dynamic (`Any`-typed)
    // receiver reaches the `CallTypedDispatchOrBuiltin` dispatch site but no
    // user `getindex`/`Base.getindex` override matches at runtime (Issue #6657).
    // Performs the same operation as the `IndexLoad` instruction.
    // Appended for bincode discriminant compatibility.
    GetIndex,
    // names(m::Module) - module binding names (Issue #7938). Appended for
    // bincode discriminant compatibility.
    Names,
    // Bool(x) numeric constructor (Issue #7971). Mirrors the Int8/Float64/...
    // constructors: routes through the range-checked `convert(Bool, x)` so only
    // 0/1 succeed (else InexactError). Appended at the end for bincode
    // discriminant compatibility.
    Bool,
}

/// Generates `BuiltinId::name` and `BuiltinId::from_name` from a single table so
/// the two directions cannot drift out of sync (Issue #6831). Each row is
/// `Variant: "canonical_name" => ["from_name", "alias", ...]`: the canonical name
/// is what `name()` returns; the bracket list is every string `from_name`
/// accepts for that variant (empty for internal/name-only builtins). Discriminant
/// order is fixed by the hand-written `enum BuiltinId` above (bincode cache
/// compatibility), not by this table. The two platform-conditional integer
/// aliases (`"Int"`/`"UInt"`, which follow the host pointer width) are not
/// table-expressible and are hardcoded in the generated `from_name`.
macro_rules! define_builtin_table {
    ( $( $variant:ident : $canon:literal => [ $( $alias:literal ),* $(,)? ] ),* $(,)? ) => {
        impl BuiltinId {
            /// Get builtin from function name.
            ///
            /// # Examples
            ///
            /// ```
            /// use subset_julia_vm::builtins::BuiltinId;
            ///
            /// assert_eq!(BuiltinId::from_name("round"), Some(BuiltinId::Round));
            /// assert_eq!(BuiltinId::from_name("unknown"), None);
            /// ```
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $( $( $alias => Some(Self::$variant), )* )*
                    // `Int`/`UInt` always alias the 64-bit integer types because
                    // the VM's integer carrier is uniformly `Int64` (Issue #7310).
                    "Int" => Some(Self::Int64),
                    "UInt" => Some(Self::UInt64),
                    _ => None,
                }
            }

            /// Get the canonical name of this builtin.
            pub fn name(&self) -> &'static str {
                match self {
                    $( Self::$variant => $canon, )*
                }
            }
        }
    };
}

define_builtin_table! {
    Sqrt: "sqrt" => [],
    Floor: "floor" => [],
    FloorDigits: "floor_digits" => [],
    FloorSigDigits: "floor_sigdigits" => [],
    Ceil: "ceil" => [],
    CeilDigits: "ceil_digits" => [],
    CeilSigDigits: "ceil_sigdigits" => [],
    Round: "round" => ["round"],
    RoundDigits: "round_digits" => [],
    RoundSigDigits: "round_sigdigits" => [],
    Trunc: "trunc" => ["trunc"],
    TruncDigits: "trunc_digits" => ["trunc_digits"],
    TruncSigDigits: "trunc_sigdigits" => ["trunc_sigdigits"],
    CountOnes: "_ctpop_int" => ["_ctpop_int"],
    LeadingZeros: "_ctlz_int" => ["_ctlz_int"],
    TrailingZeros: "_cttz_int" => ["_cttz_int"],
    Bitreverse: "_bitreverse_int" => ["_bitreverse_int"],
    Bswap: "_bswap_int" => ["_bswap_int"],
    Fma: "_fma" => ["_fma"],
    NegAny: "neg_any" => [],
    Zeros: "zeros" => ["zeros"],
    ZerosF64: "zeros_f64" => [],
    ZerosI64: "zeros_i64" => [],
    Ones: "ones" => ["ones"],
    OnesF64: "ones_f64" => [],
    OnesI64: "ones_i64" => [],
    Similar: "similar" => ["similar"],
    AllocUndefF64: "alloc_undef_f64" => [],
    AllocUndefI64: "alloc_undef_i64" => [],
    AllocUndefBool: "alloc_undef_bool" => [],
    AllocUndefAny: "alloc_undef_any" => [],
    MarkBitVector: "_mark_bitvector" => ["_mark_bitvector"],
    MarkBitArray: "_mark_bitarray" => ["_mark_bitarray"],
    Reshape: "reshape" => ["reshape"],
    Length: "length" => ["length"],
    Size: "size" => ["size"],
    Ndims: "ndims" => ["ndims"],
    Eltype: "eltype" => ["eltype"],
    Keytype: "keytype" => ["keytype"],
    Valtype: "valtype" => ["valtype"],
    MemoryRefNew: "memoryref" => ["memoryref", "memoryrefnew"],
    MemoryRefGet: "memoryrefget" => ["memoryrefget"],
    MemoryRefSet: "memoryrefset!" => ["memoryrefset!"],
    MemoryRefOffset: "memoryrefoffset" => ["memoryrefoffset"],
    MemoryRefParent: "memoryrefparent" => ["memoryrefparent"],
    Push: "push!" => ["push!"],
    Pop: "pop!" => ["pop!"],
    PushFirst: "pushfirst!" => ["pushfirst!"],
    PopFirst: "popfirst!" => ["popfirst!"],
    Insert: "insert!" => ["insert!"],
    DeleteAt: "deleteat!" => ["deleteat!"],
    Append: "append!" => ["append!"],
    Prepend: "prepend!" => ["prepend!"],
    Any: "any" => ["any"],
    All: "all" => ["all"],
    Count: "count" => ["count"],
    Compose: "compose" => ["compose"],
    RangeNew: "range" => ["range"],
    RangeCollect: "collect" => ["collect"],
    LinRange: "LinRange" => ["LinRange"],
    StringNew: "string" => ["string"],
    StringFromChars: "String" => ["String"],
    Repr: "repr" => ["repr"],
    Sprintf: "sprintf" => ["sprintf"],
    PrintfFmtFloat: "_printf_fmt_float" => ["_printf_fmt_float"],
    Ncodeunits: "ncodeunits" => ["ncodeunits"],
    Codeunit: "codeunit" => ["codeunit"],
    CodeUnits: "codeunits" => ["codeunits"],
    Occursin: "occursin" => ["occursin"],
    // StringToIntBase removed (Issue #7875) - parse(Int, s; base=N) is now Pure
    // Julia (`_parse_int_base` in base/parse.jl); the compiler rewrites the
    // kwargs form to a positional pure-Julia call.
    StringIntToBase: "string" => [],
    CharToInt: "Int" => [],
    IntToChar: "Char" => ["Char"],
    // Isnumeric removed (Issue #6752) - isnumeric is now Pure Julia
    // (base/strings/unicode.jl), routed DispatchFirst to the method table.
    SubStringRetag: "_substring_retag" => ["_substring_retag"],
    IsvalidIndex: "isvalid" => [],
    TryparseFloat64: "_tryparse_float64" => ["_tryparse_float64"],
    Print: "print" => ["print"],
    Println: "println" => ["println"],
    IOBufferNew: "IOBuffer" => ["IOBuffer"],
    TakeString: "take!" => ["take!", "takestring!"],
    IOWrite: "write" => ["write"],
    IOPrint: "print" => [],
    Displaysize: "displaysize" => ["displaysize"],
    IncludeDependency: "include_dependency" => ["include_dependency"],
    Precompile: "__precompile__" => ["__precompile__"],
    Normpath: "normpath" => ["normpath"],
    Abspath: "abspath" => ["abspath"],
    Homedir: "homedir" => ["homedir"],
    ReadFile: "read" => [],
    ReadLines: "readlines" => ["readlines"],
    Eachline: "eachline" => ["eachline"],
    Readline: "readline" => ["readline"],
    Countlines: "countlines" => ["countlines"],
    Isfile: "isfile" => ["isfile"],
    Isdir: "isdir" => ["isdir"],
    Ispath: "ispath" => ["ispath"],
    Filesize: "filesize" => ["filesize"],
    Pwd: "pwd" => ["pwd"],
    Readdir: "readdir" => ["readdir"],
    Mkdir: "mkdir" => ["mkdir"],
    Mkpath: "mkpath" => ["mkpath"],
    Rm: "rm" => ["rm"],
    Tempdir: "tempdir" => ["tempdir"],
    Tempname: "tempname" => ["tempname"],
    Touch: "touch" => ["touch"],
    Cd: "cd" => ["cd"],
    Islink: "islink" => ["islink"],
    Cp: "cp" => ["cp"],
    Mv: "mv" => ["mv"],
    Mtime: "mtime" => ["mtime"],
    Open: "open" => ["open"],
    Close: "close" => ["close"],
    Eof: "eof" => ["eof"],
    Isopen: "isopen" => ["isopen"],
    ReadlineIo: "readline" => [],
    Rand: "rand" => ["rand"],
    Randn: "randn" => ["randn"],
    RandInt: "rand" => [],
    TimeNs: "time_ns" => ["time_ns"],
    Sleep: "sleep" => ["sleep"],
    TypeOf: "typeof" => ["typeof"],
    TypeVar: "TypeVar" => ["TypeVar"],
    UnionAll: "UnionAll" => ["UnionAll"],
    Isa: "isa" => ["isa"],
    Sizeof: "sizeof" => ["sizeof"],
    Isbitstype: "isbitstype" => ["isbitstype"],
    _Supertype: "_supertype" => ["_supertype"],
    _Typename: "_typename" => ["_typename"],
    _FunctionName: "_function_name" => ["_function_name"],
    Subtypes: "subtypes" => [],
    Egal: "===" => [],
    NotEgal: "!==" => ["!=="],
    Isequal: "isequal" => ["isequal"],
    TupleEquals: "_tuple_equals" => [],
    Isless: "isless" => ["isless"],
    Hash: "hash" => ["hash"],
    Objectid: "objectid" => ["objectid"],
    Isunordered: "isunordered" => ["isunordered"],
    Subtype: "<:" => ["<:"],
    SupertypeOp: ">:" => [">:"],
    In: "in" => ["in"],
    Convert: "convert" => ["convert"],
    Promote: "promote" => ["promote"],
    Signed: "signed" => ["signed"],
    Unsigned: "unsigned" => ["unsigned"],
    FloatConv: "float" => [],
    Reinterpret: "reinterpret" => ["reinterpret"],
    Deepcopy: "deepcopy" => ["deepcopy"],
    _Fieldnames: "_fieldnames" => ["_fieldnames"],
    _Fieldtypes: "_fieldtypes" => ["_fieldtypes"],
    _Fieldoffset: "_fieldoffset" => ["_fieldoffset"],
    _DatatypeAlignment: "_datatype_alignment" => ["_datatype_alignment"],
    _Allocatedinline: "_allocatedinline" => ["_allocatedinline"],
    _Getfield: "_getfield" => ["_getfield"],
    _Isabstracttype: "_isabstracttype" => ["_isabstracttype"],
    _Isconcretetype: "_isconcretetype" => ["_isconcretetype"],
    _Ismutabletype: "_ismutabletype" => ["_ismutabletype"],
    _Isprimitivetype: "_isprimitivetype" => ["_isprimitivetype"],
    _Isstructtype: "_isstructtype" => ["_isstructtype"],
    _Typeintersect: "_typeintersect" => ["_typeintersect"],
    _TypeUnion: "_type_union" => ["_type_union"],
    _MakeTupleType: "_make_tuple_type" => ["_make_tuple_type"],
    _TypeParameters: "_type_parameters" => ["_type_parameters"],
    _Hash: "_hash" => ["_hash"],
    _Eltype: "_eltype" => ["_eltype"],
    Getfield: "getfield" => ["getfield"],
    Setfield: "setfield!" => ["setfield!"],
    _MethodsByFtype: "_methods_by_ftype" => ["_methods_by_ftype"],
    HasMethod: "hasmethod" => ["hasmethod"],
    Which: "which" => ["which"],
    Names: "names" => ["names"],
    IsExported: "isexported" => ["isexported"],
    IsPublic: "ispublic" => ["ispublic"],
    IsdefinedModuleBinding: "_isdefined_module_binding" => ["_isdefined_module_binding"],
    TupleNew: "tuple" => [],
    TupleFirst: "first" => ["first", "_tuple_first"],
    TupleLast: "last" => ["last", "_tuple_last"],
    TupleLen: "length" => [],
    DictGet: "get" => ["get"],
    DictGetkey: "getkey" => ["getkey"],
    DictSet: "setindex!" => [],
    DictDelete: "delete!" => ["delete!"],
    DictHasKey: "haskey" => ["haskey"],
    DictKeys: "keys" => ["keys"],
    DictValues: "values" => ["values"],
    DictPairs: "pairs" => ["pairs"],
    DictGetBang: "get!" => ["get!"],
    DictMergeBang: "merge!" => ["merge!"],
    DictEmpty: "empty!" => ["empty!"],
    DictPop: "pop!" => [],
    Lu: "lu" => ["lu"],
    Det: "det" => ["det"],
    Inv: "inv" => ["inv"],
    Ldiv: "\\" => ["\\"],
    Svd: "svd" => ["svd"],
    Qr: "qr" => ["qr"],
    Eigen: "eigen" => ["eigen"],
    Eigvals: "eigvals" => ["eigvals"],
    Cholesky: "cholesky" => ["cholesky"],
    Rank: "rank" => ["rank"],
    Cond: "cond" => ["cond"],
    RefNew: "Ref" => ["Ref"],
    RefUnwrap: "getindex" => [],
    Zero: "zero" => ["zero"],
    One: "one" => ["one"],
    Bool: "Bool" => ["Bool"],
    Int8: "Int8" => ["Int8"],
    Int16: "Int16" => ["Int16"],
    Int32: "Int32" => ["Int32"],
    Int64: "Int64" => ["Int64"],
    Int128: "Int128" => ["Int128"],
    UInt8: "UInt8" => ["UInt8"],
    UInt16: "UInt16" => ["UInt16"],
    UInt32: "UInt32" => ["UInt32"],
    UInt64: "UInt64" => ["UInt64"],
    UInt128: "UInt128" => ["UInt128"],
    Float16: "Float16" => ["Float16"],
    Float32: "Float32" => ["Float32"],
    Float64: "Float64" => ["Float64"],
    BigInt: "BigInt" => ["BigInt"],
    BigFloat: "BigFloat" => ["BigFloat"],
    BigFloatPrecision: "_bigfloat_precision" => ["_bigfloat_precision"],
    BigFloatDefaultPrecision: "_bigfloat_default_precision" => ["_bigfloat_default_precision"],
    SetBigFloatDefaultPrecision: "_set_bigfloat_default_precision!" => ["_set_bigfloat_default_precision!"],
    BigFloatRounding: "_bigfloat_rounding" => ["_bigfloat_rounding"],
    SetBigFloatRounding: "_set_bigfloat_rounding!" => ["_set_bigfloat_rounding!"],
    GetZeroSubnormals: "get_zero_subnormals" => ["get_zero_subnormals"],
    SetZeroSubnormals: "set_zero_subnormals" => ["set_zero_subnormals"],
    NonMissingType: "nonmissingtype" => ["nonmissingtype"],
    Iterate: "iterate" => ["iterate"],
    SymbolNew: "Symbol" => ["Symbol"],
    ExprNew: "Expr" => ["Expr"],
    ExprNewWithSplat: "Expr(with splat)" => [],
    Gensym: "gensym" => ["gensym"],
    Esc: "esc" => ["esc"],
    QuoteNodeNew: "QuoteNode" => ["QuoteNode"],
    LineNumberNodeNew: "LineNumberNode" => ["LineNumberNode"],
    GlobalRefNew: "GlobalRef" => ["GlobalRef"],
    Eval: "eval" => ["eval"],
    MetaParse: "_meta_parse" => ["_meta_parse"],
    MetaParseAt: "_meta_parse_at" => ["_meta_parse_at"],
    MetaIsExpr: "_meta_isexpr" => ["_meta_isexpr"],
    MetaQuot: "_meta_quot" => ["_meta_quot"],
    MetaIsIdentifier: "Meta.isidentifier" => ["_meta_isidentifier"],
    MetaIsOperator: "Meta.isoperator" => ["_meta_isoperator"],
    MetaIsUnaryOperator: "Meta.isunaryoperator" => ["_meta_isunaryoperator"],
    MetaIsBinaryOperator: "Meta.isbinaryoperator" => ["_meta_isbinaryoperator"],
    MetaIsPostfixOperator: "Meta.ispostfixoperator" => ["_meta_ispostfixoperator"],
    MetaLower: "_meta_lower" => ["_meta_lower"],
    MacroExpand: "macroexpand" => ["macroexpand"],
    MacroExpandBang: "macroexpand!" => ["macroexpand!"],
    IncludeString: "include_string" => ["include_string"],
    EvalFile: "evalfile" => ["evalfile"],
    TestRecord: "_test_record!" => ["_test_record!"],
    TestRecordBroken: "_test_record_broken!" => ["_test_record_broken!"],
    TestSetBegin: "_testset_begin!" => ["_testset_begin!"],
    TestSetEnd: "_testset_end!" => ["_testset_end!"],
    RegexNew: "Regex" => ["Regex"],
    RegexMatch: "match" => ["match"],
    RegexOccursin: "occursin" => [],
    RegexReplace: "_regex_replace" => ["_regex_replace"],
    RegexSplit: "split" => [],
    RegexEachmatch: "eachmatch" => ["eachmatch"],
    _UnionAllVar: "_unionall_var" => ["_unionall_var"],
    _UnionAllBody: "_unionall_body" => ["_unionall_body"],
    _TypeVarName: "_type_var_name" => ["_type_var_name"],
    _TypeVarLowerBound: "_type_var_lower_bound" => ["_type_var_lower_bound"],
    _TypeVarUpperBound: "_type_var_upper_bound" => ["_type_var_upper_bound"],
    EndsWithRegex: "_endswith_regex" => ["_endswith_regex"],
    IOBufferFromString: "IOBuffer" => [],
    ComposeExceptionType: "_compose_exception_type" => ["_compose_exception_type"],
    _ReturnTypesByFtype: "_return_types_by_ftype" => ["_return_types_by_ftype"],
    GeneratedEval: "_generated_eval" => [],
    GetIndex: "getindex" => [],
}

impl BuiltinId {
    /// Check if this builtin is a pure math function (no side effects).
    pub fn is_pure_math(&self) -> bool {
        matches!(
            self,
            // Note: Sin, Cos, Tan, Asin, Acos, Atan, Exp, Log removed — now Pure Julia (base/math.jl)
            Self::Sqrt
                | Self::Floor
                | Self::FloorDigits
                | Self::FloorSigDigits
                | Self::Ceil
                | Self::CeilDigits
                | Self::CeilSigDigits
                | Self::Round
                | Self::RoundDigits
                | Self::RoundSigDigits
                | Self::Trunc
                | Self::TruncDigits
                | Self::TruncSigDigits // Note: NextFloat/PrevFloat/NextFloatN/PrevFloatN removed - now Pure Julia (base/float.jl, Issue #6740)
                                       // Note: Gcd, Lcm, Factorial removed - now Pure Julia (base/intfuncs.jl)
        )
    }

    /// Check if this builtin has side effects (mutates state or performs I/O).
    pub fn has_side_effects(&self) -> bool {
        matches!(
            self,
            Self::Push
                | Self::Pop
                | Self::Append
                | Self::Prepend
                | Self::Print
                | Self::Println
                | Self::Sleep
                | Self::DictSet
                | Self::DictDelete
                | Self::DictGetBang
                | Self::DictMergeBang
                | Self::DictEmpty
        )
    }

    /// Check if this builtin returns a value (vs. returning nothing).
    pub fn returns_value(&self) -> bool {
        // Note: ForEach removed - foreach is now Pure Julia (base/abstractarray.jl)
        !matches!(
            self,
            Self::Print | Self::Println | Self::Sleep | Self::IncludeDependency | Self::Precompile
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_name_math() {
        // sin, cos, tan, asin, acos, atan, exp, log removed — now Pure Julia
        assert_eq!(BuiltinId::from_name("sin"), None);
        assert_eq!(BuiltinId::from_name("cos"), None);
        assert_eq!(BuiltinId::from_name("exp"), None);
        assert_eq!(BuiltinId::from_name("log"), None);
        // Rounding functions still present
        assert_eq!(BuiltinId::from_name("round"), Some(BuiltinId::Round));
        assert_eq!(BuiltinId::from_name("trunc"), Some(BuiltinId::Trunc));
    }

    #[test]
    fn test_from_name_array() {
        assert_eq!(BuiltinId::from_name("zeros"), Some(BuiltinId::Zeros));
        assert_eq!(BuiltinId::from_name("length"), Some(BuiltinId::Length));
        assert_eq!(BuiltinId::from_name("push!"), Some(BuiltinId::Push));
    }

    #[test]
    fn test_from_name_unknown() {
        assert_eq!(BuiltinId::from_name("unknown_function"), None);
    }

    #[test]
    fn test_name_roundtrip() {
        let builtins = [
            BuiltinId::Round,
            BuiltinId::Trunc,
            BuiltinId::Zeros,
            BuiltinId::Length,
        ];

        for builtin in builtins {
            let name = builtin.name();
            assert_eq!(BuiltinId::from_name(name), Some(builtin));
        }
    }

    // ---- `define_builtin_table!` macro edge cases (Issue #6831) ----

    #[test]
    fn test_table_int_aliases_are_always_64_bit() {
        // `Int`/`UInt` always alias the 64-bit variants because the VM's integer
        // carrier is uniformly `Int64`, independent of host pointer width (Issue
        // #7310). On a 32-bit target this used to resolve to `Int32` and break
        // dispatch against the `Int64` literals it was compared with.
        assert_eq!(BuiltinId::from_name("Int"), Some(BuiltinId::Int64));
        assert_eq!(BuiltinId::from_name("UInt"), Some(BuiltinId::UInt64));
        // Explicit widths are unconditional.
        assert_eq!(BuiltinId::from_name("Int64"), Some(BuiltinId::Int64));
        assert_eq!(BuiltinId::from_name("Int32"), Some(BuiltinId::Int32));
    }

    #[test]
    fn test_table_name_vs_from_name_asymmetry() {
        // `Meta.*` predicates: `name()` returns the dotted form, but `from_name`
        // only accepts the underscored intrinsic spelling.
        assert_eq!(BuiltinId::MetaIsIdentifier.name(), "Meta.isidentifier");
        assert_eq!(
            BuiltinId::from_name("_meta_isidentifier"),
            Some(BuiltinId::MetaIsIdentifier)
        );
        assert_eq!(BuiltinId::from_name("Meta.isidentifier"), None);
        // `CharToInt` is name-only: its canonical name is "Int" but `from_name`
        // never maps "Int" to it (that goes to the integer constructor).
        assert_eq!(BuiltinId::CharToInt.name(), "Int");
        assert_ne!(BuiltinId::from_name("Int"), Some(BuiltinId::CharToInt));
    }

    #[test]
    fn test_table_name_only_variants_and_aliases() {
        // Name-only variants have a `name()` but `from_name` returns None.
        assert_eq!(BuiltinId::Floor.name(), "floor");
        assert_eq!(BuiltinId::from_name("floor"), None);
        // Multi-string aliases map to the same variant.
        assert_eq!(
            BuiltinId::from_name("memoryref"),
            BuiltinId::from_name("memoryrefnew")
        );
        assert_eq!(
            BuiltinId::from_name("memoryref"),
            Some(BuiltinId::MemoryRefNew)
        );
    }

    #[test]
    fn test_is_pure_math() {
        // Sin, Cos removed — now Pure Julia
        assert!(BuiltinId::Floor.is_pure_math());
        assert!(BuiltinId::Round.is_pure_math());
        assert!(!BuiltinId::Print.is_pure_math());
        assert!(!BuiltinId::Push.is_pure_math());
    }

    #[test]
    fn test_has_side_effects() {
        assert!(!BuiltinId::Floor.has_side_effects());
        assert!(BuiltinId::Print.has_side_effects());
        assert!(BuiltinId::Push.has_side_effects());
    }
}
