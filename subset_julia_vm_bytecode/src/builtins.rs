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

/// Built-in function identifiers.
///
/// These functions are implemented in Rust and called via `CallBuiltin` instruction.
/// Unlike Intrinsics (CPU-level operations), Builtins are higher-level library functions.
// `strum::VariantNames` feeds `compile::precompile::enum_variant_fingerprint()`
// (Issue #8626): `BuiltinId` is part of the serialized bytecode wire format
// (payload of `Instr::CallBuiltin`), so variant insert/remove/reorder must be
// detected at cache load time.
//
// Serialize/Deserialize are implemented in `compile::instr_wire_ids` via stable
// wire IDs (Issue #8627) — not derived, to decouple declaration order from
// the wire representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::VariantNames)]
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
    RangeStep,
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
    StringNew,       // _string(...) - VM formatting fallback for Julia wrappers
    StringFromChars, // _string_from_chars(chars) - VM string storage boundary (Issue #2038/#8780)
    Repr,            // repr(x)
    Sprintf,         // sprintf(fmt, args...) - formatted string
    PrintfFmtFloat, // _printf_fmt_float(x, conv::Char, prec::Int) - C float→string boundary (Issue #6746)

    // String query methods
    Ncodeunits, // ncodeunits(s) - number of bytes
    Codeunit,   // codeunit(s, i) - get byte at position i
    CodeUnits,  // legacy _codeunits carrier; public codeunits is a Julia CodeUnits wrapper

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
    StringIntToBase, // legacy _string_int_base carrier; public string(n; base) is Julia (Issue #8780)
    CharToInt,       // _char_to_int(c) - char to codepoint
    // Codepoint removed - pure Julia (Issue #6747)
    IntToChar, // _int_to_char(n) - codepoint to char
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
    TakeString,  // take!(io) - extract bytes from IOBuffer
    IOWrite,     // write(io, x) - write to IOBuffer
    IOPrint,     // print(io, args...) - print multiple args to IO, returns nothing
    // Boundary: condition 1 (process stdio is an OS boundary; the VM's
    // stdout/stderr sinks that redirect_stdout/redirect_stderr swap are
    // Rust-side VM state, and Pipe wraps the same Rust IO sink object),
    // Issue #9577 (+#10034). Retro-added by the Issue #9696 drift audit.
    PipeNew,        // Pipe() - create a minimal pipe IO object
    RedirectStdout, // redirect_stdout(io) / redirect_stdout(f, io)
    RedirectStderr, // redirect_stderr(io) / redirect_stderr(f, io)
    Displaysize,    // displaysize() - return terminal size as (rows, cols)

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
    Seek,       // seek(io, pos) - move file cursor to absolute byte position
    Position,   // position(io) - current file cursor byte position
    Skip,       // skip(io, n) - move file cursor relative to current position
    Flush,      // flush(io) - flush pending writes
    ReadCharIo, // read(io, Char) - read one character from IO stream

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
    // Boundary: condition 3 (module binding table reflection), Issue #876.
    IsPublic, // ispublic(m::Module, s::Symbol) - check if symbol is public (Julia 1.11+)
    IsdefinedModuleBinding, // _isdefined_module_binding(m::Module, s::Symbol) - check module binding (Issue #5002/#4958)
    // Boundary: condition 3 (Core.Binding field state — value/globalref — lives
    // in the VM-internal module binding representation), Issue #10067.
    // Retro-added by the Issue #10247 drift attribution.
    IsdefinedBindingField, // _isdefined_binding_field(b::Core.Binding, s::Symbol) - check Core.Binding field is set (Issue #10067)
    // Boundary: condition 3 (the module's unqualified name lives in the
    // VM-internal ModuleValue representation, same reflection boundary as
    // IsPublic/IsdefinedModuleBinding), Issue #11171.
    _ModuleName, // _module_name(m::Module) - module's own unqualified name symbol, backs nameof(::Module) (Issue #11171)

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
    RegexSplit,     // _regex_split(string, regex, limit, keepempty) - split string by regex
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
    // Boundary: condition 3 (VM module registry metadata), Issue #7938.
    // names(m::Module) - module binding names. Appended for bincode
    // discriminant compatibility.
    Names,
    // Bool(x) constructor removed from Rust (Issue #8768): now Pure Julia in
    // base/bool.jl, matching upstream `Bool(x::Real)`.
    // _compose_effects(f, types) — body-derived effect summary for matched user
    // methods (Issue #8441). Appended at the end for bincode compatibility.
    // Boundary: condition 3 (compiler method/IR effect metadata), Issue #8441.
    ComposeEffects,
    // Compiler-internal trampoline body for a method defined by runtime
    // `eval` of a quoted function-definition `Expr` (Issue #8647). Looks up
    // the currently executing function's stored body in the VM's
    // eval-defined-method registry and re-enters the tree-walking `eval`
    // interpreter against the frame's already-bound parameter slots — the
    // runtime-`eval` analog of `GeneratedEval` above. Appended at the end
    // for bincode discriminant compatibility.
    // Boundary: condition 3 (eval-internal representation needed to define a
    // method from a runtime `Expr`), Issue #8647.
    EvalDefinedCall,
    // Backtrace inspection helpers. Appended at the end for bincode
    // discriminant compatibility (Issue #8993).
    Backtrace,
    CatchBacktrace,
    Stacktrace,
    // _bigfloat_nextfloat(x::BigFloat, up::Bool) — one-ULP step of a BigFloat at
    // its own precision (nextfloat when up, prevfloat when !up), mirroring MPFR's
    // mpfr_nextabove / mpfr_nextbelow. The generic AbstractFloat nextfloat routes
    // through Float64 `reinterpret`, which cannot apply to a BigFloat (Issue
    // #9280). Appended at the end for bincode discriminant compatibility.
    BigFloatNextfloat,
    // _bigfloat_get_exp(x::BigFloat) — the base-2 exponent E of a finite nonzero
    // BigFloat where x = m·2^E with m ∈ [0.5, 1) (the MPFR mpfr_get_exp / astro
    // exponent convention). Backs exponent/frexp/significand of BigFloat, whose
    // generic AbstractFloat definitions route through Float64 `reinterpret`
    // (Issue #9286). Appended at the end for bincode discriminant compatibility.
    BigFloatGetExp,
    // _bigfloat_scale2(x::BigFloat, n::Int64) — x · 2^n computed exactly by
    // shifting the astro exponent (no rounding). Used with a normalizing shift
    // (n = -E or 1-E) to extract the frexp mantissa / significand of a BigFloat
    // (Issue #9286). Appended at the end for bincode discriminant compatibility.
    BigFloatScale2,
    // _display_artifact(x) — host-display hook for the multimedia display stack
    // (Issue #9262). Under a graphical host (iOS/web REPL/Editor) that has an
    // artifact-capable display active, tries to render `x` as a display artifact
    // (the same `try_value_to_artifact` path the trailing-expression rendering
    // uses) and, on success, buffers it in the VM display sink and returns
    // `true`; otherwise returns `false` so pure-Julia `display` falls back to
    // text output (CLI/script parity with a headless Julia session). Appended at
    // the end for bincode discriminant compatibility.
    EmitDisplayArtifact,
    // _bigfloat_signbit(x::BigFloat) — the sign bit of a BigFloat read from the
    // astro sign field, so a negative zero is observable (the generic
    // `signbit(x) = x < 0` cannot see it, mis-signing abs/copysign/mod of
    // BigFloat zeros). NaN reports `false`, matching MPFR/Julia's default NaN
    // sign (Issue #9450). Appended at the end for bincode discriminant
    // compatibility.
    BigFloatSignbit,
    // _linspace_range_f64(start, stop, len) — construct the TwicePrecision-
    // backed float range for `range(start, stop; length = len)` on
    // Float64-representable endpoints, upstream
    // `range_start_stop_length(::T, ::T, ::Integer) where T<:IEEEFloat`
    // (julia/base/twiceprecision.jl `_linspace`). Returns a length-defined
    // `Value::Range` whose `typeof` is `StepRangeLen{Float64,
    // Base.TwicePrecision{Float64}, Base.TwicePrecision{Float64}, Int64}`
    // (Issue #9419). Appended at the end for bincode discriminant
    // compatibility.
    LinspaceF64,
    // _try_complex_scale_tp_range_f64(re, im, r) — upstream range broadcast
    // fusion `x::Complex .* r::StepRangeLen{Float64, TwicePrecision,
    // TwicePrecision}` (julia/base/broadcast.jl:1169): materialize the
    // complex-scaled TwicePrecision range with upstream-bit-identical element
    // values, or return `nothing` when `r` is not a TwicePrecision-backed
    // Float64 range (Issue #9659). Appended at the end for bincode
    // discriminant compatibility.
    // Boundary: condition 3 (the TwicePrecision hi/lo parts of a native
    // `Value::Range` are VM-internal representation not reachable from Pure
    // Julia; the scaled-lerp element math must run on them), Issue #9659.
    ComplexScaleTpRange,
    // _try_broadcast_typed_kernel(f, args...) — bulk typed-kernel broadcast
    // (Issues #9693/#8797): dispatch `f` once, then run its frame-less typed
    // scalar function block over the array's raw storage in one Rust loop.
    // Returns `nothing` when not applicable (generic broadcast fallback).
    // Boundary: condition 4 (no-JIT perf boundary: per-element interpreted
    // dispatch/boxing/frames are the broadcast bottleneck; the block executor
    // and raw `ArrayData` access are VM-internal), Issue #8797.
    BroadcastTypedKernel,
    // _try_broadcast_binary_arith(f, a, b) — upstream-exact elementwise
    // `+`/`-`/`*` over numeric/complex array broadcasts, dispatched once and
    // executed as one Rust loop (Issue #8797). Returns `nothing` when not
    // applicable (generic broadcast fallback). Boundary: condition 4 (same
    // no-JIT broadcast bottleneck as BroadcastTypedKernel).
    BroadcastBinaryArith,
    // _type_equal(a, b) — semantic type equality for Pure Julia
    // `==(::Type, ::Type)`. Appended at the end for bincode discriminant
    // compatibility (Issue #9563).
    // Boundary: condition 3 (runtime type-object equality depends on VM-internal
    // DataType/UnionAll/TypeVar representation), Issue #9563.
    _TypeEqual,
    // Weak references and finalizer/GC hooks. Appended for bincode
    // discriminant compatibility (Issue #8990).
    // Boundary: condition 3 (WeakRef cells, finalizer registration, and the
    // gc_* hooks observe/mutate the VM's Rc-based memory-management internals
    // — liveness, weak upgrades, finalizer queues — which have no Pure Julia
    // representation), Issue #8990. Retro-added by the Issue #9696 drift audit.
    WeakRefNew,
    WeakRefValue,
    WeakRefSetValue,
    Finalizer,
    Finalize,
    GcCollect,
    GcSafepoint,
    GcInFinalizer,
    // _test_record_error!(msg, detail) — record an errored test outcome
    // (exception thrown or non-Boolean value while evaluating a `@test`
    // expression), mirroring upstream `Test.Error` / `do_test`'s `Threw`
    // branch (Issue #10093). Appended at the end (NOT grouped with the other
    // Test operations above) for bincode discriminant compatibility.
    // Boundary: condition 3 (testset counters, the sticky any_test_failed exit
    // flag, and the summary printer live in VM-internal session state),
    // Issue #10093. Retro-added by the Issue #10247/#10256 drift attribution.
    TestRecordError,
    // _regex_findnext(regex, string, i) — first match of `regex` at or after
    // 1-based byte index `i`, returned as a RegexMatch (or Nothing). Backs the
    // pure-Julia findnext(::Regex, s, i) / findfirst(::Regex, s) methods
    // (Issue #10177), mirroring upstream `_findnext_re`'s PCRE.exec(re, str,
    // idx-1) positional search. Appended at the end for bincode discriminant
    // compatibility. Boundary: condition 3 (positional matching against the
    // VM-internal fancy_regex engine, preserving full-string lookbehind/`\b`/`^`
    // context — the same engine boundary as the other regex builtins).
    RegexFindnext,

    // SubstitutionString capture-reference expansion for `replace(s, re => s"…")`
    // (Issue #10174). Appended at the end for bincode discriminant compatibility
    // with existing precompiled Base caches.
    ExpandSubstitution, // _expand_substitution(subst, match, regex) - expand \1 / \g<name> / \0

    // findnext(re, str, i) primitive for the multi-pattern `replace` scan
    // (Issue #10175). Appended at the end for bincode discriminant compatibility.
    RegexMatchFrom, // _regex_match_from(regex, string, byteindex) - first match at/after i

    // Cooperative task scheduler boundaries (Issue #10349). Public Task,
    // Channel, wait, yield, and schedule semantics remain Pure Julia; these
    // six operations only transfer VM-owned continuation/session state.
    // Boundary: condition 3 (VM frame/stack continuation state), Issue #10349
    TaskRegisterMain,
    // Boundary: condition 3 (VM frame/stack continuation state), Issue #10349
    TaskSchedule,
    // Boundary: condition 3 (VM frame/stack continuation state), Issue #10349
    TaskYield,
    // Boundary: condition 3 (VM frame/stack continuation state), Issue #10349
    TaskPark,
    // Boundary: condition 3 (VM frame/stack continuation state), Issue #10349
    TaskWake,
    // Boundary: condition 3 (VM frame/stack continuation state), Issue #10349
    TaskCurrent,

    // _steprangelen_range_f64(start, step, len, tag) — construct the
    // TwicePrecision-backed float range for `range(start; step, length = len)`
    // (upstream `range_start_step_length(::T, ::T, ::Integer) where
    // T<:IEEEFloat`, julia/base/twiceprecision.jl:448). `tag` selects the
    // element type (0 = Float64, 1 = Float32, 2 = Float16); narrow-float tags
    // collapse ref/step to plain Float64 scalars, matching upstream
    // `StepRangeLen{Float32/Float16, Float64, Float64, Int64}` (Issue #9509).
    // Boundary: condition 3 (same TwicePrecision VM-internal representation
    // as LinspaceF64, Issue #9419). Appended at the end for bincode
    // discriminant compatibility.
    SteprangelenF64,
    /// `_throw_method_error_with_args(args..., message, fname)` — raise the
    /// compile-time-detected dispatch miss with its typed payload (the actual
    /// argument values plus the callable name) so a caught `MethodError`
    /// exposes upstream's `.f`/`.args` instead of a rendered string
    /// (Issue #11374). Appended at the end for bincode discriminant
    /// compatibility.
    ThrowMethodErrorWithArgs,
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
    NegAny: "_neg_any" => ["_neg_any"],
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
    Compose: "_compose" => ["_compose"],
    RangeNew: "range" => ["range"],
    RangeCollect: "collect" => ["collect"],
    RangeStep: "_range_step" => ["_range_step"],
    LinRange: "LinRange" => ["LinRange"],
    StringNew: "_string" => ["_string"],
    StringFromChars: "_string_from_chars" => ["_string_from_chars"],
    Repr: "repr" => ["repr"],
    Sprintf: "sprintf" => ["sprintf"],
    PrintfFmtFloat: "_printf_fmt_float" => ["_printf_fmt_float"],
    Ncodeunits: "ncodeunits" => ["ncodeunits"],
    Codeunit: "codeunit" => ["codeunit"],
    CodeUnits: "_codeunits" => [],
    Occursin: "occursin" => ["occursin"],
    // StringToIntBase removed (Issue #7875) - parse(Int, s; base=N) is now Pure
    // Julia (`_parse_int_base` in base/parse.jl); the compiler rewrites the
    // kwargs form to a positional pure-Julia call.
    StringIntToBase: "_string_int_base" => [],
    CharToInt: "_char_to_int" => ["_char_to_int"],
    IntToChar: "_int_to_char" => ["_int_to_char"],
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
    PipeNew: "Pipe" => ["Pipe"],
    RedirectStdout: "redirect_stdout" => ["redirect_stdout"],
    RedirectStderr: "redirect_stderr" => ["redirect_stderr"],
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
    Seek: "seek" => ["seek"],
    Position: "position" => ["position"],
    Skip: "skip" => ["skip"],
    Flush: "flush" => ["flush"],
    ReadCharIo: "read" => [],
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
    NotEgal: "_not_egal" => ["_not_egal"],
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
    Deepcopy: "_deepcopy" => ["_deepcopy"],
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
    IsdefinedBindingField: "_isdefined_binding_field" => ["_isdefined_binding_field"],
    _ModuleName: "_module_name" => ["_module_name"],
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
    RefNew: "_ref_new" => ["_ref_new"],
    RefUnwrap: "_ref_get" => ["_ref_get"],
    Zero: "zero" => ["zero"],
    One: "one" => ["one"],
    Int8: "_to_int8" => ["_to_int8"],
    Int16: "_to_int16" => ["_to_int16"],
    Int32: "_to_int32" => ["_to_int32"],
    Int64: "Int64" => ["Int64"],
    Int128: "_to_int128" => ["_to_int128"],
    UInt8: "_to_uint8" => ["_to_uint8"],
    UInt16: "_to_uint16" => ["_to_uint16"],
    UInt32: "_to_uint32" => ["_to_uint32"],
    UInt64: "_to_uint64" => ["_to_uint64"],
    UInt128: "_to_uint128" => ["_to_uint128"],
    Float16: "_to_float16" => ["_to_float16"],
    Float32: "_to_float32" => ["_to_float32"],
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
    NonMissingType: "_nonmissingtype" => ["_nonmissingtype"],
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
    RegexSplit: "_regex_split" => ["_regex_split"],
    RegexEachmatch: "eachmatch" => ["eachmatch"],
    _UnionAllVar: "_unionall_var" => ["_unionall_var"],
    _UnionAllBody: "_unionall_body" => ["_unionall_body"],
    _TypeVarName: "_type_var_name" => ["_type_var_name"],
    _TypeVarLowerBound: "_type_var_lower_bound" => ["_type_var_lower_bound"],
    _TypeVarUpperBound: "_type_var_upper_bound" => ["_type_var_upper_bound"],
    EndsWithRegex: "_endswith_regex" => ["_endswith_regex"],
    IOBufferFromString: "IOBuffer" => [],
    ComposeExceptionType: "_compose_exception_type" => ["_compose_exception_type"],
    ComposeEffects: "_compose_effects" => ["_compose_effects"],
    _ReturnTypesByFtype: "_return_types_by_ftype" => ["_return_types_by_ftype"],
    GeneratedEval: "_generated_eval" => [],
    GetIndex: "getindex" => [],
    EvalDefinedCall: "_eval_defined_call" => [],
    Backtrace: "_sjulia_backtrace" => ["_sjulia_backtrace"],
    CatchBacktrace: "_sjulia_catch_backtrace" => ["_sjulia_catch_backtrace"],
    Stacktrace: "_sjulia_stacktrace" => ["_sjulia_stacktrace"],
    BigFloatNextfloat: "_bigfloat_nextfloat" => ["_bigfloat_nextfloat"],
    BigFloatGetExp: "_bigfloat_get_exp" => ["_bigfloat_get_exp"],
    BigFloatScale2: "_bigfloat_scale2" => ["_bigfloat_scale2"],
    EmitDisplayArtifact: "_display_artifact" => ["_display_artifact"],
    BigFloatSignbit: "_bigfloat_signbit" => ["_bigfloat_signbit"],
    LinspaceF64: "_linspace_range_f64" => ["_linspace_range_f64"],
    ComplexScaleTpRange: "_try_complex_scale_tp_range_f64" => ["_try_complex_scale_tp_range_f64"],
    BroadcastTypedKernel: "_try_broadcast_typed_kernel" => ["_try_broadcast_typed_kernel"],
    BroadcastBinaryArith: "_try_broadcast_binary_arith" => ["_try_broadcast_binary_arith"],
    _TypeEqual: "_type_equal" => ["_type_equal"],
    WeakRefNew: "_weakref_new" => ["_weakref_new"],
    WeakRefValue: "_weakref_value" => ["_weakref_value"],
    WeakRefSetValue: "_weakref_set_value!" => ["_weakref_set_value!"],
    Finalizer: "_finalizer" => ["_finalizer"],
    Finalize: "_finalize" => ["_finalize"],
    GcCollect: "_gc_collect" => ["_gc_collect"],
    GcSafepoint: "_gc_safepoint" => ["_gc_safepoint"],
    GcInFinalizer: "_gc_in_finalizer" => ["_gc_in_finalizer"],
    TestRecordError: "_test_record_error!" => ["_test_record_error!"],
    RegexFindnext: "_regex_findnext" => ["_regex_findnext"],
    ExpandSubstitution: "_expand_substitution" => ["_expand_substitution"],
    RegexMatchFrom: "_regex_match_from" => ["_regex_match_from"],
    TaskRegisterMain: "_task_register_main" => ["_task_register_main"],
    TaskSchedule: "_task_schedule" => ["_task_schedule"],
    TaskYield: "_task_yield" => ["_task_yield"],
    TaskPark: "_task_park" => ["_task_park"],
    TaskWake: "_task_wake" => ["_task_wake"],
    TaskCurrent: "_task_current" => ["_task_current"],
    SteprangelenF64: "_steprangelen_range_f64" => ["_steprangelen_range_f64"],
    ThrowMethodErrorWithArgs: "_throw_method_error_with_args" => [],
}

/// Upstream namespace authority for BuiltinId aliases that cannot be decided
/// from Base's export table alone (Issue #11410).
///
/// `None` means the caller must still consult Base exports: the alias is
/// either a Base-exported binding or a VM-internal spelling. Keeping the two
/// exceptional classes next to the canonical BuiltinId name table prevents
/// reflection from treating every executable VM primitive as a Julia module
/// binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinBindingAuthority {
    Core,
    BasePrivate,
}

impl BuiltinId {
    /// Return namespace authority that cannot be derived from Base exports.
    pub fn binding_authority(name: &str) -> Option<BuiltinBindingAuthority> {
        if matches!(
            name,
            "memoryref"
                | "memoryrefnew"
                | "memoryrefget"
                | "memoryrefset!"
                | "memoryrefoffset"
                | "print"
                | "println"
                | "write"
                | "typeof"
                | "TypeVar"
                | "UnionAll"
                | "isa"
                | "sizeof"
                | "<:"
                | ">:"
                | "convert"
                | "getfield"
                | "setfield!"
                | "Int64"
                | "Float64"
                | "iterate"
                | "Symbol"
                | "Expr"
                | "QuoteNode"
                | "LineNumberNode"
                | "GlobalRef"
                | "eval"
        ) {
            Some(BuiltinBindingAuthority::Core)
        } else if matches!(
            name,
            "_string" | "_fieldnames" | "_methods_by_ftype" | "isexported" | "ispublic"
        ) {
            Some(BuiltinBindingAuthority::BasePrivate)
        } else {
            None
        }
    }
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
                | Self::IOWrite
                | Self::IOPrint
                | Self::RedirectStdout
                | Self::RedirectStderr
                | Self::Sleep
                | Self::Open
                | Self::Close
                | Self::ReadlineIo
                | Self::Seek
                | Self::Skip
                | Self::Flush
                | Self::ReadCharIo
                | Self::DictSet
                | Self::DictDelete
                | Self::DictGetBang
                | Self::DictMergeBang
                | Self::DictEmpty
                | Self::WeakRefNew
                | Self::WeakRefSetValue
                | Self::Finalizer
                | Self::Finalize
                | Self::GcCollect
                | Self::GcSafepoint
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
    fn builtin_binding_authority_separates_upstream_namespaces_11410() {
        assert_eq!(
            BuiltinId::binding_authority("sizeof"),
            Some(BuiltinBindingAuthority::Core)
        );
        assert_eq!(
            BuiltinId::binding_authority("_string"),
            Some(BuiltinBindingAuthority::BasePrivate)
        );
        assert_eq!(BuiltinId::binding_authority("_ctpop_int"), None);
        assert_eq!(BuiltinId::binding_authority("zeros"), None);

        for name in ["sizeof", "_string", "Int64", "memoryref"] {
            assert!(
                BuiltinId::from_name(name).is_some(),
                "authority metadata must refer to a canonical BuiltinId alias: {name}"
            );
        }
    }

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
        // The remaining public 64-bit constructor is unconditional.
        assert_eq!(BuiltinId::from_name("Int64"), Some(BuiltinId::Int64));
        // Other fixed-width public constructors are pure Julia wrappers over
        // underscored conversion boundaries (Issue #8777).
        assert_eq!(BuiltinId::from_name("Int32"), None);
        assert_eq!(BuiltinId::from_name("_to_int32"), Some(BuiltinId::Int32));
    }

    #[test]
    fn test_numeric_constructor_public_aliases_removed_8777() {
        // Issue #8777: public fixed-width numeric constructors are pure Julia
        // wrappers. Rust keeps only underscored conversion boundaries.
        let migrated_public_names = [
            "Int8", "Int16", "Int32", "Int128", "UInt8", "UInt16", "UInt32", "UInt64", "UInt128",
            "Float16", "Float32",
        ];
        for name in migrated_public_names {
            assert_eq!(BuiltinId::from_name(name), None, "{name} stayed public");
        }

        let internal_boundaries = [
            ("_to_int8", BuiltinId::Int8),
            ("_to_int16", BuiltinId::Int16),
            ("_to_int32", BuiltinId::Int32),
            ("_to_int128", BuiltinId::Int128),
            ("_to_uint8", BuiltinId::UInt8),
            ("_to_uint16", BuiltinId::UInt16),
            ("_to_uint32", BuiltinId::UInt32),
            ("_to_uint64", BuiltinId::UInt64),
            ("_to_uint128", BuiltinId::UInt128),
            ("_to_float16", BuiltinId::Float16),
            ("_to_float32", BuiltinId::Float32),
        ];
        for (name, builtin) in internal_boundaries {
            assert_eq!(BuiltinId::from_name(name), Some(builtin));
        }

        assert_eq!(BuiltinId::from_name("!=="), None);
        assert_eq!(BuiltinId::from_name("_not_egal"), Some(BuiltinId::NotEgal));
        assert_eq!(BuiltinId::from_name("neg_any"), None);
        assert_eq!(BuiltinId::from_name("_neg_any"), Some(BuiltinId::NegAny));
    }

    #[test]
    fn test_ref_compose_missing_copy_public_aliases_removed_8779() {
        // Issue #8779: these public names are ordinary Julia methods. Rust keeps
        // only underscored primitive boundaries for the pure wrappers.
        let migrated_public_names = ["Ref", "compose", "nonmissingtype", "deepcopy"];
        for name in migrated_public_names {
            assert_eq!(BuiltinId::from_name(name), None, "{name} stayed public");
        }

        let internal_boundaries = [
            ("_ref_new", BuiltinId::RefNew),
            ("_ref_get", BuiltinId::RefUnwrap),
            ("_compose", BuiltinId::Compose),
            ("_nonmissingtype", BuiltinId::NonMissingType),
            ("_deepcopy", BuiltinId::Deepcopy),
        ];
        for (name, builtin) in internal_boundaries {
            assert_eq!(BuiltinId::from_name(name), Some(builtin));
        }
    }

    #[test]
    fn test_string_char_public_aliases_removed_8780() {
        // Issue #8780: public string/char constructor names are ordinary Julia
        // methods. Rust keeps only underscored primitive boundaries for wrappers
        // that still bottom out in VM-owned string/char storage.
        let migrated_public_names = ["string", "String", "codeunits", "Char"];
        for name in migrated_public_names {
            assert_eq!(BuiltinId::from_name(name), None, "{name} stayed public");
        }

        let internal_boundaries = [
            ("_string", BuiltinId::StringNew),
            ("_string_from_chars", BuiltinId::StringFromChars),
            ("_char_to_int", BuiltinId::CharToInt),
            ("_int_to_char", BuiltinId::IntToChar),
        ];
        for (name, builtin) in internal_boundaries {
            assert_eq!(BuiltinId::from_name(name), Some(builtin));
        }
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
        // `Int` still resolves to the native Int64 constructor; the public
        // `Int(::Char)` method reaches the char boundary through `_char_to_int`.
        assert_eq!(BuiltinId::CharToInt.name(), "_char_to_int");
        assert_ne!(BuiltinId::from_name("Int"), Some(BuiltinId::CharToInt));
        assert_eq!(
            BuiltinId::from_name("_char_to_int"),
            Some(BuiltinId::CharToInt)
        );
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
