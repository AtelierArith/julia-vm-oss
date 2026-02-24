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
    NextFloat, // nextfloat(x) - smallest float > x
    PrevFloat, // prevfloat(x) - largest float < x

    // Bit operations (integer intrinsics)
    CountOnes,     // popcount - number of 1 bits
    CountZeros,    // number of 0 bits (bitwidth - count_ones)
    LeadingZeros,  // leading zero bits
    LeadingOnes,   // leading 1 bits
    TrailingZeros, // trailing zero bits
    TrailingOnes,  // trailing 1 bits
    Bitreverse,    // reverse all bits
    Bitrotate,     // rotate bits left (positive k) or right (negative k)
    Bswap,         // byte swap (reverse byte order)

    // Float decomposition (IEEE 754)
    Exponent,    // exponent(x) - get exponent part of float
    Significand, // significand(x) - get significand (mantissa) part
    Frexp,       // frexp(x) - returns (mantissa, exponent) tuple

    // Float inspection
    Issubnormal, // issubnormal(x) - check if subnormal number
    Maxintfloat, // maxintfloat() - max integer representable as Float64

    // Fused multiply-add
    Fma,    // fma(x, y, z) = x*y + z (fused, single rounding)
    Muladd, // muladd(x, y, z) = x*y + z (may or may not be fused)

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
    ZerosF64,        // zeros(Float64, dims...) - create Float64 array
    ZerosI64,        // zeros(Int64, dims...) - create Int64 array
    ZerosComplexF64, // zeros(Complex{Float64}, dims...) - create ComplexF64 array
    Ones,
    OnesF64, // ones(Float64, dims...) - create Float64 array
    OnesI64, // ones(Int64, dims...) - create Int64 array
    // Note: Trues, Falses, Fill are now Pure Julia (base/array.jl) — Issue #2640
    Similar,
    // Uninitialized array allocation: Vector{T}(undef, n), Array{T}(undef, dims...)
    AllocUndefF64, // Array{Float64}(undef, dims...) - create uninitialized Float64 array
    AllocUndefI64, // Array{Int64}(undef, dims...) - create uninitialized Int64 array
    AllocUndefComplexF64, // Array{Complex{Float64}}(undef, dims...) - create uninitialized ComplexF64 array
    AllocUndefBool,       // Array{Bool}(undef, dims...) - create uninitialized Bool array
    AllocUndefAny,        // Array{Any}(undef, dims...) - create uninitialized Any array
    // Copy: Now implemented in Pure Julia (base/array.jl)
    Reshape,

    // Query
    Length,
    Size,
    Ndims,
    Eltype,
    Keytype,
    Valtype,

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
    Sort,

    // Aggregation
    // Sum: Now Pure Julia (base/array.jl)
    Prod,
    Minimum,
    Maximum,
    // Mean: Now Pure Julia (stdlib/Statistics/src/Statistics.jl)

    // Statistics: All now Pure Julia (stdlib/Statistics/src/Statistics.jl)
    // Var, Varm, Std, Stdm, Median, Middle, Cov, Cor, Quantile

    // Search
    Argmin,
    Argmax,
    FindFirst,
    FindAll,

    // =========================================================================
    // Higher-Order Functions
    // =========================================================================
    // Note: map, filter, reduce, foldl, foldr, foreach are now Pure Julia
    Any,
    All,
    Count,
    Ntuple,
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
    Complex, // complex(re, im)
    // Note: Real, Imag, Conj, Angle removed - now Pure Julia (Issue #2640)

    // =========================================================================
    // String Operations
    // =========================================================================
    StringNew,       // string(...)
    StringFromChars, // String(::Vector{Char}) - char array to string (Issue #2038)
    Repr,            // repr(x)
    Sprintf,         // sprintf(fmt, args...) - formatted string

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
    StringToFloat,   // parse(Float64, s)
    StringToIntBase, // parse(Int, s; base=N) - kwargs variant kept as Rust builtin
    StringIntToBase, // string(x; base=N) (Issue #2036)
    CharToInt,       // Int(c) - char to codepoint
    Codepoint,       // codepoint(c) - Unicode codepoint as UInt32
    IntToChar,       // Char(n) - codepoint to char
    Bitstring,       // bitstring(x) - binary representation as string
    // Ascii removed - now Pure Julia in base/strings/util.jl
    // Nextind, Prevind, Thisind, Reverseind removed - now Pure Julia (base/strings/basic.jl)
    // Bytes2Hex, Hex2Bytes removed - now Pure Julia (base/strings/util.jl)
    UnescapeString, // unescape_string(s) - unescape string escape sequences
    Isnumeric,      // isnumeric(c) - check if character is numeric (Unicode)
    IsvalidIndex,   // isvalid(s, i) - check if index is valid character boundary
    // FindNextString, FindPrevString removed - now Pure Julia (base/strings/search.jl)
    // TryparseInt64 removed - now Pure Julia (base/parse.jl)
    TryparseFloat64, // tryparse(Float64, s) - parse string as Float64, return nothing on failure
    StringCount, // count(pattern, string) - count non-overlapping occurrences of pattern in string (Issue #2009)
    StringFindAll, // findall(pattern, string) - find all non-overlapping occurrences as Vector{UnitRange} (Issue #2013)

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
    TypeOf,        // typeof(x)
    Isa,           // isa(x, T)
    Sizeof,        // sizeof(x) - size in bytes
    Isbits,        // isbits(x) - is x a bits type instance
    Isbitstype,    // isbitstype(T) - is T a bits type
    Supertype,     // supertype(T) - get parent type
    Supertypes,    // supertypes(T) - tuple of all supertypes
    Subtypes,      // subtypes(T) - vector of direct subtypes
    Typeintersect, // typeintersect(A, B) - type intersection
    // Typejoin removed - now Pure Julia (base/reflection.jl)
    // Fieldcount removed - now Pure Julia (base/reflection.jl)
    Hasfield, // hasfield(T, name) - check if field exists
    // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
    // removed - now Pure Julia (base/reflection.jl) with _Isabstracttype/_Isconcretetype/_Ismutabletype intrinsics
    Ismutable, // ismutable(x) - is x mutable
    // NameOf removed - now Pure Julia (base/reflection.jl)

    // =========================================================================
    // Object Identity / Equality
    // =========================================================================
    Egal,        // === (object identity)
    NotEgal,     // !== (object non-identity)
    Isequal,     // isequal(x, y) - NaN-aware equality
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
    Convert,     // convert(T, x) - convert x to type T
    Promote,     // promote(x, y, ...) - promote to common type
    Signed,      // signed(x) - convert to signed integer (same bit width)
    Unsigned,    // unsigned(x) - convert to unsigned integer (same bit width)
    FloatConv,   // float(x) - convert to Float64
    Widemul,     // widemul(a, b) - wide multiplication (no overflow)
    Reinterpret, // reinterpret(T, x) - bit-level type reinterpretation

    // =========================================================================
    // Copy Operations
    // =========================================================================
    Deepcopy, // deepcopy(x) - recursive deep copy

    // =========================================================================
    // Reflection / Introspection (Internal VM builtins)
    // =========================================================================
    _Fieldnames,     // _fieldnames(T) - tuple of field names (internal)
    _Fieldtypes,     // _fieldtypes(T) - tuple of field types (internal)
    _Getfield,       // _getfield(x, i) - get field value by index (internal)
    _Isabstracttype, // _isabstracttype(T) - check abstract type (internal)
    _Isconcretetype, // _isconcretetype(T) - check concrete type (internal)
    _Ismutabletype,  // _ismutabletype(T) - check mutable type (internal)
    _Hash,           // _hash(x) - compute hash value (internal, Issue #2582)
    _Eltype,         // _eltype(x) - get element type (internal, Issue #2570)
    _DictGet,        // _dict_get(d, key) - HashMap lookup (internal, Issue #2572)
    _DictSet,        // _dict_set!(d, key, value) - HashMap insert (internal, Issue #2572)
    _DictDelete,     // _dict_delete!(d, key) - HashMap remove (internal, Issue #2572)
    _DictHaskey,     // _dict_haskey(d, key) - HashMap contains_key (internal, Issue #2572)
    _DictLength,     // _dict_length(d) - HashMap len (internal, Issue #2572)
    _DictEmpty,      // _dict_empty!(d) - HashMap clear (internal, Issue #2572)
    _DictKeys,       // _dict_keys(d) - HashMap keys as Tuple (internal, Issue #2669)
    _DictValues,     // _dict_values(d) - HashMap values as Tuple (internal, Issue #2669)
    _DictPairs,      // _dict_pairs(d) - HashMap entries as Tuple of Tuples (internal, Issue #2669)
    _SetPush,        // _set_push!(s, x) - HashSet insert (internal, Issue #2574)
    _SetDelete,      // _set_delete!(s, x) - HashSet remove (internal, Issue #2574)
    _SetIn,          // _set_in(x, s) - HashSet contains (internal, Issue #2574)
    _SetEmpty,       // _set_empty!(s) - HashSet clear (internal, Issue #2574)
    _SetLength,      // _set_length(s) - HashSet len (internal, Issue #2574)
    Getfield,        // getfield(x, name) or getfield(x, i) - get field by name or index
    Setfield,        // setfield!(x, name, v) or setfield!(x, i, v) - set field by name or index
    Methods,         // methods(f) or methods(f, types) - list of methods
    HasMethod,       // hasmethod(f, types) - check if method exists
    Which,           // which(f, types) - get specific method
    IsExported,      // isexported(m::Module, s::Symbol) - check if symbol is exported
    IsPublic,        // ispublic(m::Module, s::Symbol) - check if symbol is public (Julia 1.11+)

    // =========================================================================
    // Tuple/Dict Operations
    // =========================================================================
    TupleNew,
    TupleFirst,
    TupleLast,
    TupleLen,

    DictNew,
    DictGet,       // get(dict, key) or get(dict, key, default)
    DictGetkey,    // getkey(dict, key, default) - return key if exists, else default
    DictSet,       // setindex!(dict, value, key) or dict[key] = value
    DictDelete,    // delete!(dict, key)
    DictHasKey,    // haskey(dict, key)
    DictLen,       // length(dict) - specialized for dict
    DictKeys,      // keys(dict)
    DictValues,    // values(dict)
    DictPairs,     // pairs(dict)
    DictMerge,     // merge(dict1, dict2)
    DictGetBang,   // get!(dict, key, default) - get or insert default
    DictMergeBang, // merge!(dict1, dict2) - merge in-place
    DictEmpty,     // empty!(dict) - remove all entries
    DictPop,       // pop!(dict, key) or pop!(dict, key, default) - remove and return value

    // =========================================================================
    // Set Operations
    // =========================================================================
    SetNew,          // Set() or Set([...]) - create set
    SetPush,         // push!(set, x) - add element to set
    SetDelete,       // delete!(set, x) - remove element from set
    SetIn,           // in(x, set) or x ∈ set - check membership
    SetUnion,        // union(a, b) or a ∪ b - set union
    SetIntersect,    // intersect(a, b) or a ∩ b - set intersection
    SetSetdiff,      // setdiff(a, b) - set difference
    SetSymdiff,      // symdiff(a, b) - symmetric difference
    SetIssubset,     // issubset(a, b) or a ⊆ b - subset check
    SetIsdisjoint,   // isdisjoint(a, b) - disjoint check
    SetIssetequal,   // issetequal(a, b) - set equality check
    SetEmpty,        // empty!(set) - remove all elements
    SetUnionMut,     // union!(s, itr) - add elements from itr to s in-place
    SetIntersectMut, // intersect!(s, itr) - keep only elements also in itr
    SetSetdiffMut,   // setdiff!(s, itr) - remove elements found in itr
    SetSymdiffMut,   // symdiff!(s, itr) - symmetric difference in-place

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
}

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
            // Math - Trigonometric: Now implemented in Pure Julia (base/math.jl)
            // sin, cos, tan, asin, acos, atan

            // Math - Hyperbolic: Now implemented in Pure Julia (base/math.jl)
            // sinh, cosh, tanh, asinh, acosh, atanh

            // Math - Exponential/Logarithmic: Now implemented in Pure Julia (base/math.jl)
            // exp, log

            // Math - Rounding
            "round" => Some(Self::Round),
            "trunc" => Some(Self::Trunc),
            "trunc_digits" => Some(Self::TruncDigits),
            "trunc_sigdigits" => Some(Self::TruncSigDigits),
            // Float adjacency
            "nextfloat" => Some(Self::NextFloat),
            "prevfloat" => Some(Self::PrevFloat),
            // Bit operations
            "count_ones" => Some(Self::CountOnes),
            "count_zeros" => Some(Self::CountZeros),
            "leading_zeros" => Some(Self::LeadingZeros),
            "leading_ones" => Some(Self::LeadingOnes),
            "trailing_zeros" => Some(Self::TrailingZeros),
            "trailing_ones" => Some(Self::TrailingOnes),
            "bitreverse" => Some(Self::Bitreverse),
            "bitrotate" => Some(Self::Bitrotate),
            "bswap" => Some(Self::Bswap),
            // Float decomposition
            "exponent" => Some(Self::Exponent),
            "significand" => Some(Self::Significand),
            "frexp" => Some(Self::Frexp),
            // Float inspection
            "issubnormal" => Some(Self::Issubnormal),
            "maxintfloat" => Some(Self::Maxintfloat),
            // Fused multiply-add
            "fma" => Some(Self::Fma),
            "muladd" => Some(Self::Muladd),
            // Number theory - now Pure Julia (base/intfuncs.jl)
            // gcd, lcm, factorial removed

            // Array - Creation
            "zeros" => Some(Self::Zeros),
            "ones" => Some(Self::Ones),
            // trues, falses, fill are now Pure Julia (base/array.jl) — Issue #2640
            "similar" => Some(Self::Similar),
            "reshape" => Some(Self::Reshape),
            // copy: Now implemented in Pure Julia (base/array.jl)

            // Array - Query
            "length" => Some(Self::Length),
            "size" => Some(Self::Size),
            "ndims" => Some(Self::Ndims),
            "eltype" => Some(Self::Eltype),
            "keytype" => Some(Self::Keytype),
            "valtype" => Some(Self::Valtype),

            // Array - Manipulation
            "push!" => Some(Self::Push),
            "pop!" => Some(Self::Pop),
            "pushfirst!" => Some(Self::PushFirst),
            "popfirst!" => Some(Self::PopFirst),
            "insert!" => Some(Self::Insert),
            "deleteat!" => Some(Self::DeleteAt),
            "append!" => Some(Self::Append),
            "prepend!" => Some(Self::Prepend),
            // reverse: Now implemented in Pure Julia (base/array.jl)
            "sort" => Some(Self::Sort),

            // Array - Aggregation: Now implemented in Pure Julia (base/array.jl)
            // sum, prod, minimum, maximum, mean

            // Array - Search: argmin, argmax now in Pure Julia (base/array.jl)
            "findfirst" => Some(Self::FindFirst),
            "findall" => Some(Self::FindAll),

            // Higher-Order Functions
            // Note: map, filter, reduce, foldl, foldr, foreach are now Pure Julia
            "any" => Some(Self::Any),
            "all" => Some(Self::All),
            "count" => Some(Self::Count),
            "ntuple" => Some(Self::Ntuple),
            "compose" => Some(Self::Compose),

            // Range
            "range" => Some(Self::RangeNew),
            "collect" => Some(Self::RangeCollect),
            "LinRange" => Some(Self::LinRange),

            // Complex
            "complex" => Some(Self::Complex),
            // Note: real, imag, conj, angle removed - now Pure Julia — Issue #2640

            // String
            "string" => Some(Self::StringNew),
            "String" => Some(Self::StringFromChars),
            "repr" => Some(Self::Repr),
            "sprintf" => Some(Self::Sprintf),
            "ncodeunits" => Some(Self::Ncodeunits),
            "codeunit" => Some(Self::Codeunit),
            "codeunits" => Some(Self::CodeUnits),
            // "uppercase", "lowercase", "titlecase" now Pure Julia - removed builtins
            // strip, lstrip, rstrip, chomp, chop now Pure Julia - removed builtins
            "occursin" => Some(Self::Occursin),
            // "split", "rsplit" now Pure Julia - removed builtins
            // "repeat" now Pure Julia - removed builtin
            "Char" => Some(Self::IntToChar),
            "codepoint" => Some(Self::Codepoint),
            "bitstring" => Some(Self::Bitstring),
            // "ascii" now Pure Julia - removed builtin
            // nextind, prevind, thisind, reverseind now Pure Julia (base/strings/basic.jl)
            // bytes2hex, hex2bytes now Pure Julia (base/strings/util.jl)
            "unescape_string" => Some(Self::UnescapeString),
            "isnumeric" => Some(Self::Isnumeric),
            // "findnext", "findprev" now Pure Julia (base/strings/search.jl)
            // tryparse is handled at compile-time with type dispatch

            // I/O
            "print" => Some(Self::Print),
            "println" => Some(Self::Println),
            "IOBuffer" => Some(Self::IOBufferNew),
            "take!" => Some(Self::TakeString),
            "takestring!" => Some(Self::TakeString),
            "write" => Some(Self::IOWrite),
            "displaysize" => Some(Self::Displaysize),
            "include_dependency" => Some(Self::IncludeDependency),
            "__precompile__" => Some(Self::Precompile),
            // Note: dirname, basename, joinpath, splitext, splitdir, isabspath, isdirpath
            // are now Pure Julia (base/path.jl) — Issue #2637
            "normpath" => Some(Self::Normpath),
            "abspath" => Some(Self::Abspath),
            "homedir" => Some(Self::Homedir),

            // File I/O
            "readlines" => Some(Self::ReadLines),
            "readline" => Some(Self::Readline),
            "countlines" => Some(Self::Countlines),
            "isfile" => Some(Self::Isfile),
            "isdir" => Some(Self::Isdir),
            "ispath" => Some(Self::Ispath),
            "filesize" => Some(Self::Filesize),
            "pwd" => Some(Self::Pwd),
            "readdir" => Some(Self::Readdir),
            "mkdir" => Some(Self::Mkdir),
            "mkpath" => Some(Self::Mkpath),
            "rm" => Some(Self::Rm),
            "tempdir" => Some(Self::Tempdir),
            "tempname" => Some(Self::Tempname),
            "touch" => Some(Self::Touch),
            "cd" => Some(Self::Cd),
            "islink" => Some(Self::Islink),
            "cp" => Some(Self::Cp),
            "mv" => Some(Self::Mv),
            "mtime" => Some(Self::Mtime),
            "open" => Some(Self::Open),
            "close" => Some(Self::Close),
            "eof" => Some(Self::Eof),
            "isopen" => Some(Self::Isopen),

            // RNG
            "rand" => Some(Self::Rand),
            "randn" => Some(Self::Randn),

            // Time
            "time_ns" => Some(Self::TimeNs),
            "sleep" => Some(Self::Sleep),

            // Type
            "typeof" => Some(Self::TypeOf),
            "isa" => Some(Self::Isa),
            "sizeof" => Some(Self::Sizeof),
            "isbits" => Some(Self::Isbits),
            "isbitstype" => Some(Self::Isbitstype),
            "supertype" => Some(Self::Supertype),
            // "fieldcount" removed - now Pure Julia (base/reflection.jl)
            "hasfield" => Some(Self::Hasfield),
            // isconcretetype, isabstracttype, isprimitivetype, isstructtype removed
            // now Pure Julia (base/reflection.jl)
            "ismutable" => Some(Self::Ismutable),
            // "ismutabletype" removed - now Pure Julia (base/reflection.jl)
            // "nameof" removed - now Pure Julia (base/reflection.jl)

            // Object Identity / Equality
            "isequal" => Some(Self::Isequal),
            "isless" => Some(Self::Isless),
            "hash" => Some(Self::Hash),
            "objectid" => Some(Self::Objectid),
            "isunordered" => Some(Self::Isunordered),
            "!==" => Some(Self::NotEgal),
            ">:" => Some(Self::SupertypeOp),

            // Set Operations
            "in" => Some(Self::In),

            // Type Conversion
            "convert" => Some(Self::Convert),
            "promote" => Some(Self::Promote),
            "signed" => Some(Self::Signed),
            "unsigned" => Some(Self::Unsigned),
            "float" => Some(Self::FloatConv),
            "widemul" => Some(Self::Widemul),
            "reinterpret" => Some(Self::Reinterpret),

            // Copy Operations
            "deepcopy" => Some(Self::Deepcopy),

            // Reflection / Introspection (internal builtins)
            "_fieldnames" => Some(Self::_Fieldnames),
            "_fieldtypes" => Some(Self::_Fieldtypes),
            "_getfield" => Some(Self::_Getfield),
            "_hash" => Some(Self::_Hash),
            "_eltype" => Some(Self::_Eltype),
            "_isabstracttype" => Some(Self::_Isabstracttype),
            "_isconcretetype" => Some(Self::_Isconcretetype),
            "_ismutabletype" => Some(Self::_Ismutabletype),

            // Dict internal intrinsics (Issue #2572)
            "_dict_get" => Some(Self::_DictGet),
            "_dict_set!" => Some(Self::_DictSet),
            "_dict_delete!" => Some(Self::_DictDelete),
            "_dict_haskey" => Some(Self::_DictHaskey),
            "_dict_length" => Some(Self::_DictLength),
            "_dict_empty!" => Some(Self::_DictEmpty),
            "_dict_keys" => Some(Self::_DictKeys),
            "_dict_values" => Some(Self::_DictValues),
            "_dict_pairs" => Some(Self::_DictPairs),
            // Set internal intrinsics (Issue #2574)
            "_set_push!" => Some(Self::_SetPush),
            "_set_delete!" => Some(Self::_SetDelete),
            "_set_in" => Some(Self::_SetIn),
            "_set_empty!" => Some(Self::_SetEmpty),
            "_set_length" => Some(Self::_SetLength),
            "getfield" => Some(Self::Getfield),
            "setfield!" => Some(Self::Setfield),
            "methods" => Some(Self::Methods),
            "hasmethod" => Some(Self::HasMethod),
            "which" => Some(Self::Which),
            "isexported" => Some(Self::IsExported),
            "ispublic" => Some(Self::IsPublic),

            // Tuple
            "first" => Some(Self::TupleFirst),
            "last" => Some(Self::TupleLast),

            // Dict
            "Dict" => Some(Self::DictNew),
            "get" => Some(Self::DictGet),
            "getkey" => Some(Self::DictGetkey),
            "delete!" => Some(Self::DictDelete),
            "haskey" => Some(Self::DictHasKey),
            "keys" => Some(Self::DictKeys),
            "values" => Some(Self::DictValues),
            "pairs" => Some(Self::DictPairs),
            "merge" => Some(Self::DictMerge),
            "get!" => Some(Self::DictGetBang),
            "merge!" => Some(Self::DictMergeBang),
            "empty!" => Some(Self::DictEmpty),

            // Set
            "Set" => Some(Self::SetNew),
            "union" => Some(Self::SetUnion),
            "intersect" => Some(Self::SetIntersect),
            "setdiff" => Some(Self::SetSetdiff),
            "symdiff" => Some(Self::SetSymdiff),
            "issubset" => Some(Self::SetIssubset),
            "isdisjoint" => Some(Self::SetIsdisjoint),
            "issetequal" => Some(Self::SetIssetequal),
            "union!" => Some(Self::SetUnionMut),
            "intersect!" => Some(Self::SetIntersectMut),
            "setdiff!" => Some(Self::SetSetdiffMut),
            "symdiff!" => Some(Self::SetSymdiffMut),

            // Note: transpose and adjoint are now Pure Julia functions
            // (no longer mapped to builtins)

            // Linear algebra (via faer library)
            "lu" => Some(Self::Lu),
            "det" => Some(Self::Det),
            "inv" => Some(Self::Inv),
            "\\" => Some(Self::Ldiv),
            "svd" => Some(Self::Svd),
            "qr" => Some(Self::Qr),
            "eigen" => Some(Self::Eigen),
            "eigvals" => Some(Self::Eigvals),
            "cholesky" => Some(Self::Cholesky),
            "rank" => Some(Self::Rank),
            "cond" => Some(Self::Cond),

            // Broadcast control
            "Ref" => Some(Self::RefNew),

            // Zero/One
            "zero" => Some(Self::Zero),
            "one" => Some(Self::One),

            // Numeric Type Constructors
            "Int8" => Some(Self::Int8),
            "Int16" => Some(Self::Int16),
            "Int32" => Some(Self::Int32),
            "Int64" => Some(Self::Int64),
            "Int128" => Some(Self::Int128),
            "UInt8" => Some(Self::UInt8),
            "UInt16" => Some(Self::UInt16),
            "UInt32" => Some(Self::UInt32),
            "UInt64" => Some(Self::UInt64),
            "UInt128" => Some(Self::UInt128),
            "Float16" => Some(Self::Float16),
            "Float32" => Some(Self::Float32),
            "Float64" => Some(Self::Float64),

            // BigInt
            "BigInt" => Some(Self::BigInt),

            // BigFloat
            "BigFloat" => Some(Self::BigFloat),
            "_bigfloat_precision" => Some(Self::BigFloatPrecision),
            "_bigfloat_default_precision" => Some(Self::BigFloatDefaultPrecision),
            "_set_bigfloat_default_precision!" => Some(Self::SetBigFloatDefaultPrecision),
            "_bigfloat_rounding" => Some(Self::BigFloatRounding),
            "_set_bigfloat_rounding!" => Some(Self::SetBigFloatRounding),

            // Subnormal Float Control
            "get_zero_subnormals" => Some(Self::GetZeroSubnormals),
            "set_zero_subnormals" => Some(Self::SetZeroSubnormals),

            // Missing Value Utilities
            "nonmissingtype" => Some(Self::NonMissingType),

            // Iterator Protocol
            "iterate" => Some(Self::Iterate),
            // Note: "collect" is handled by RangeCollect above

            // Macro System
            "Symbol" => Some(Self::SymbolNew),
            "Expr" => Some(Self::ExprNew),
            "gensym" => Some(Self::Gensym),
            "esc" => Some(Self::Esc),
            "QuoteNode" => Some(Self::QuoteNodeNew),
            "LineNumberNode" => Some(Self::LineNumberNodeNew),
            "GlobalRef" => Some(Self::GlobalRefNew),
            "eval" => Some(Self::Eval),
            "_meta_parse" => Some(Self::MetaParse),
            "_meta_parse_at" => Some(Self::MetaParseAt),
            "_meta_isexpr" => Some(Self::MetaIsExpr),
            "_meta_quot" => Some(Self::MetaQuot),
            "_meta_isidentifier" => Some(Self::MetaIsIdentifier),
            "_meta_isoperator" => Some(Self::MetaIsOperator),
            "_meta_isunaryoperator" => Some(Self::MetaIsUnaryOperator),
            "_meta_isbinaryoperator" => Some(Self::MetaIsBinaryOperator),
            "_meta_ispostfixoperator" => Some(Self::MetaIsPostfixOperator),
            "_meta_lower" => Some(Self::MetaLower),
            "macroexpand" => Some(Self::MacroExpand),
            "macroexpand!" => Some(Self::MacroExpandBang),
            "include_string" => Some(Self::IncludeString),
            "evalfile" => Some(Self::EvalFile),

            // Test operations
            "_test_record!" => Some(Self::TestRecord),
            "_test_record_broken!" => Some(Self::TestRecordBroken),
            "_testset_begin!" => Some(Self::TestSetBegin),
            "_testset_end!" => Some(Self::TestSetEnd),

            // Regex operations
            "Regex" => Some(Self::RegexNew),
            "match" => Some(Self::RegexMatch),
            "eachmatch" => Some(Self::RegexEachmatch),
            "_regex_replace" => Some(Self::RegexReplace),

            _ => None,
        }
    }

    /// Get the canonical name of this builtin.
    pub fn name(&self) -> &'static str {
        match self {
            // Math - Trigonometric: Now Pure Julia (base/math.jl)
            // Math - Exponential/Logarithmic: Now Pure Julia (base/math.jl)

            // Math - Rounding
            Self::Floor => "floor",
            Self::FloorDigits => "floor_digits",
            Self::FloorSigDigits => "floor_sigdigits",
            Self::Ceil => "ceil",
            Self::CeilDigits => "ceil_digits",
            Self::CeilSigDigits => "ceil_sigdigits",
            Self::Round => "round",
            Self::RoundDigits => "round_digits",
            Self::RoundSigDigits => "round_sigdigits",
            Self::Trunc => "trunc",
            Self::TruncDigits => "trunc_digits",
            Self::TruncSigDigits => "trunc_sigdigits",
            // Float adjacency
            Self::NextFloat => "nextfloat",
            Self::PrevFloat => "prevfloat",
            // Bit operations
            Self::CountOnes => "count_ones",
            Self::CountZeros => "count_zeros",
            Self::LeadingZeros => "leading_zeros",
            Self::LeadingOnes => "leading_ones",
            Self::TrailingZeros => "trailing_zeros",
            Self::TrailingOnes => "trailing_ones",
            Self::Bitreverse => "bitreverse",
            Self::Bitrotate => "bitrotate",
            Self::Bswap => "bswap",
            // Float decomposition
            Self::Exponent => "exponent",
            Self::Significand => "significand",
            Self::Frexp => "frexp",
            // Float inspection
            Self::Issubnormal => "issubnormal",
            Self::Maxintfloat => "maxintfloat",
            // Fused multiply-add
            Self::Fma => "fma",
            Self::Muladd => "muladd",

            // Note: Abs is now Pure Julia

            // Unary negation with runtime dispatch
            Self::NegAny => "neg_any",

            // Note: gcd, lcm, factorial removed - now Pure Julia (base/intfuncs.jl)

            // Array
            Self::Zeros => "zeros",
            Self::ZerosF64 => "zeros_f64",
            Self::ZerosI64 => "zeros_i64",
            Self::ZerosComplexF64 => "zeros_complex_f64",
            Self::Ones => "ones",
            Self::OnesF64 => "ones_f64",
            Self::OnesI64 => "ones_i64",
            Self::Similar => "similar",
            Self::AllocUndefF64 => "alloc_undef_f64",
            Self::AllocUndefI64 => "alloc_undef_i64",
            Self::AllocUndefComplexF64 => "alloc_undef_complex_f64",
            Self::AllocUndefBool => "alloc_undef_bool",
            Self::AllocUndefAny => "alloc_undef_any",
            // Copy: Now Pure Julia (base/array.jl)
            Self::Reshape => "reshape",
            Self::Length => "length",
            Self::Size => "size",
            Self::Ndims => "ndims",
            Self::Eltype => "eltype",
            Self::Keytype => "keytype",
            Self::Valtype => "valtype",
            Self::Push => "push!",
            Self::Pop => "pop!",
            Self::PushFirst => "pushfirst!",
            Self::PopFirst => "popfirst!",
            Self::Insert => "insert!",
            Self::DeleteAt => "deleteat!",
            Self::Append => "append!",
            Self::Prepend => "prepend!",
            // Reverse: Now Pure Julia (base/array.jl, base/sort.jl)
            Self::Sort => "sort",
            // Sum: Now Pure Julia (base/array.jl)
            Self::Prod => "prod",
            Self::Minimum => "minimum",
            Self::Maximum => "maximum",
            // Statistics: Now Pure Julia (stdlib/Statistics/src/Statistics.jl)
            // Mean, Var, Varm, Std, Stdm, Median, Middle, Cov, Cor, Quantile
            Self::Argmin => "argmin",
            Self::Argmax => "argmax",
            Self::FindFirst => "findfirst",
            Self::FindAll => "findall",

            // Higher-Order
            // Note: map, filter, reduce, foldl, foldr, foreach are now Pure Julia
            Self::Any => "any",
            Self::All => "all",
            Self::Count => "count",
            Self::Ntuple => "ntuple",

            // Range
            Self::RangeNew => "range",
            Self::RangeCollect => "collect",
            Self::LinRange => "LinRange",

            // Complex
            Self::Complex => "complex",

            // String
            Self::StringNew => "string",
            Self::StringFromChars => "String",
            Self::Repr => "repr",
            Self::Sprintf => "sprintf",
            Self::Ncodeunits => "ncodeunits",
            Self::Codeunit => "codeunit",
            Self::CodeUnits => "codeunits",
            // StringFirst removed - now Pure Julia
            // StringLast removed - now Pure Julia
            // Uppercase, Lowercase, Titlecase removed - now Pure Julia (base/strings/unicode.jl)
            // Strip, Lstrip, Rstrip, Chomp, Chop removed - now Pure Julia
            Self::Occursin => "occursin",
            // Findfirst, Findlast removed - now Pure Julia (base/strings/search.jl)
            // StringSplit, StringRsplit removed - now Pure Julia
            // StringRepeat removed - now Pure Julia
            // StringReverse removed - now Pure Julia
            // StringToInt removed - now Pure Julia (base/parse.jl)
            Self::StringToFloat => "parse",
            Self::StringToIntBase => "parse",
            Self::StringIntToBase => "string",
            Self::CharToInt => "Int",
            Self::Codepoint => "codepoint",
            Self::IntToChar => "Char",
            Self::Bitstring => "bitstring",
            // Ascii, Nextind, Prevind, Thisind, Reverseind removed - now Pure Julia
            // Bytes2Hex, Hex2Bytes removed - now Pure Julia (base/strings/util.jl)
            Self::UnescapeString => "unescape_string",
            Self::Isnumeric => "isnumeric",
            Self::IsvalidIndex => "isvalid",
            // FindNextString, FindPrevString removed - now Pure Julia
            // TryparseInt64 removed - now Pure Julia (base/parse.jl)
            Self::TryparseFloat64 => "tryparse",
            Self::StringCount => "count",
            Self::StringFindAll => "findall",

            // I/O
            Self::Print => "print",
            Self::Println => "println",
            Self::IOBufferNew => "IOBuffer",
            Self::TakeString => "take!",
            Self::IOWrite => "write",
            Self::IOPrint => "print",
            Self::Displaysize => "displaysize",
            Self::IncludeDependency => "include_dependency",
            Self::Precompile => "__precompile__",
            Self::Normpath => "normpath",
            Self::Abspath => "abspath",
            Self::Homedir => "homedir",

            // File I/O
            Self::ReadFile => "read",
            Self::ReadLines => "readlines",
            Self::Readline => "readline",
            Self::Countlines => "countlines",
            Self::Isfile => "isfile",
            Self::Isdir => "isdir",
            Self::Ispath => "ispath",
            Self::Filesize => "filesize",
            Self::Pwd => "pwd",
            Self::Readdir => "readdir",
            Self::Mkdir => "mkdir",
            Self::Mkpath => "mkpath",
            Self::Rm => "rm",
            Self::Tempdir => "tempdir",
            Self::Tempname => "tempname",
            Self::Touch => "touch",
            Self::Cd => "cd",
            Self::Islink => "islink",
            Self::Cp => "cp",
            Self::Mv => "mv",
            Self::Mtime => "mtime",
            Self::Open => "open",
            Self::Close => "close",
            Self::Eof => "eof",
            Self::Isopen => "isopen",
            Self::ReadlineIo => "readline",

            // RNG
            Self::Rand => "rand",
            Self::Randn => "randn",
            Self::RandInt => "rand",

            // Time
            Self::TimeNs => "time_ns",
            Self::Sleep => "sleep",

            // Type
            Self::TypeOf => "typeof",
            Self::Isa => "isa",
            Self::Sizeof => "sizeof",
            Self::Isbits => "isbits",
            Self::Isbitstype => "isbitstype",
            Self::Supertype => "supertype",
            Self::Supertypes => "supertypes",
            Self::Subtypes => "subtypes",
            Self::Typeintersect => "typeintersect",
            // Self::Typejoin removed - now Pure Julia (base/reflection.jl)
            // Self::Fieldcount removed - now Pure Julia (base/reflection.jl)
            Self::Hasfield => "hasfield",
            // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype removed
            // now Pure Julia (base/reflection.jl)
            Self::Ismutable => "ismutable",
            // Self::Ismutabletype removed - now Pure Julia (base/reflection.jl)
            // Self::NameOf removed - now Pure Julia (base/reflection.jl)

            // Object Identity / Equality
            Self::Egal => "===",
            Self::NotEgal => "!==",
            Self::Isequal => "isequal",
            Self::Isless => "isless",
            Self::Objectid => "objectid",
            Self::Isunordered => "isunordered",
            Self::Hash => "hash",
            Self::SupertypeOp => ">:",
            Self::Subtype => "<:",

            // Set Operations
            Self::In => "in",

            // Type Conversion
            Self::Convert => "convert",
            Self::Promote => "promote",
            Self::Signed => "signed",
            Self::Unsigned => "unsigned",
            Self::FloatConv => "float",
            Self::Widemul => "widemul",
            Self::Reinterpret => "reinterpret",

            // Copy Operations
            Self::Deepcopy => "deepcopy",

            // Reflection / Introspection (internal builtins)
            Self::_Fieldnames => "_fieldnames",
            Self::_Fieldtypes => "_fieldtypes",
            Self::_Getfield => "_getfield",
            Self::_Hash => "_hash",
            Self::_Eltype => "_eltype",
            Self::_Isabstracttype => "_isabstracttype",
            Self::_Isconcretetype => "_isconcretetype",
            Self::_Ismutabletype => "_ismutabletype",
            Self::_DictGet => "_dict_get",
            Self::_DictSet => "_dict_set!",
            Self::_DictDelete => "_dict_delete!",
            Self::_DictHaskey => "_dict_haskey",
            Self::_DictLength => "_dict_length",
            Self::_DictEmpty => "_dict_empty!",
            Self::_DictKeys => "_dict_keys",
            Self::_DictValues => "_dict_values",
            Self::_DictPairs => "_dict_pairs",
            Self::_SetPush => "_set_push!",
            Self::_SetDelete => "_set_delete!",
            Self::_SetIn => "_set_in",
            Self::_SetEmpty => "_set_empty!",
            Self::_SetLength => "_set_length",
            Self::Getfield => "getfield",
            Self::Setfield => "setfield!",
            Self::Methods => "methods",
            Self::HasMethod => "hasmethod",
            Self::Which => "which",
            Self::IsExported => "isexported",
            Self::IsPublic => "ispublic",

            // Tuple
            Self::TupleNew => "tuple",
            Self::TupleFirst => "first",
            Self::TupleLast => "last",
            Self::TupleLen => "length",

            // Dict
            Self::DictNew => "Dict",
            Self::DictGet => "get",
            Self::DictGetkey => "getkey",
            Self::DictSet => "setindex!",
            Self::DictDelete => "delete!",
            Self::DictHasKey => "haskey",
            Self::DictLen => "length",
            Self::DictKeys => "keys",
            Self::DictValues => "values",
            Self::DictPairs => "pairs",
            Self::DictMerge => "merge",
            Self::DictGetBang => "get!",
            Self::DictMergeBang => "merge!",
            Self::DictEmpty => "empty!",
            Self::DictPop => "pop!",

            // Set
            Self::SetNew => "Set",
            Self::SetPush => "push!",
            Self::SetDelete => "delete!",
            Self::SetIn => "in",
            Self::SetUnion => "union",
            Self::SetIntersect => "intersect",
            Self::SetSetdiff => "setdiff",
            Self::SetSymdiff => "symdiff",
            Self::SetIssubset => "issubset",
            Self::SetIsdisjoint => "isdisjoint",
            Self::SetIssetequal => "issetequal",
            Self::SetEmpty => "empty!",
            Self::SetUnionMut => "union!",
            Self::SetIntersectMut => "intersect!",
            Self::SetSetdiffMut => "setdiff!",
            Self::SetSymdiffMut => "symdiff!",

            // Note: Transpose and Adjoint removed - now Pure Julia

            // Linear algebra (via faer library)
            Self::Lu => "lu",
            Self::Det => "det",
            Self::Inv => "inv",
            Self::Ldiv => "\\",
            Self::Svd => "svd",
            Self::Qr => "qr",
            Self::Eigen => "eigen",
            Self::Eigvals => "eigvals",
            Self::Cholesky => "cholesky",
            Self::Rank => "rank",
            Self::Cond => "cond",

            // Broadcast control
            Self::RefNew => "Ref",
            Self::RefUnwrap => "getindex",

            // Zero/One
            Self::Zero => "zero",
            Self::One => "one",

            // Numeric Type Constructors
            Self::Int8 => "Int8",
            Self::Int16 => "Int16",
            Self::Int32 => "Int32",
            Self::Int64 => "Int64",
            Self::Int128 => "Int128",
            Self::UInt8 => "UInt8",
            Self::UInt16 => "UInt16",
            Self::UInt32 => "UInt32",
            Self::UInt64 => "UInt64",
            Self::UInt128 => "UInt128",
            Self::Float16 => "Float16",
            Self::Float32 => "Float32",
            Self::Float64 => "Float64",

            // BigInt
            Self::BigInt => "BigInt",

            // BigFloat
            Self::BigFloat => "BigFloat",
            Self::BigFloatPrecision => "_bigfloat_precision",
            Self::BigFloatDefaultPrecision => "_bigfloat_default_precision",
            Self::SetBigFloatDefaultPrecision => "_set_bigfloat_default_precision!",
            Self::BigFloatRounding => "_bigfloat_rounding",
            Self::SetBigFloatRounding => "_set_bigfloat_rounding!",

            // Subnormal Float Control
            Self::GetZeroSubnormals => "get_zero_subnormals",
            Self::SetZeroSubnormals => "set_zero_subnormals",

            // Missing Value Utilities
            Self::NonMissingType => "nonmissingtype",

            // Iterator Protocol
            Self::Iterate => "iterate",
            // Note: Collect is handled by RangeCollect

            // Macro System
            Self::SymbolNew => "Symbol",
            Self::ExprNew => "Expr",
            Self::ExprNewWithSplat => "Expr(with splat)",
            Self::Gensym => "gensym",
            Self::Esc => "esc",
            Self::QuoteNodeNew => "QuoteNode",
            Self::LineNumberNodeNew => "LineNumberNode",
            Self::GlobalRefNew => "GlobalRef",
            Self::Eval => "eval",
            Self::MetaParse => "_meta_parse",
            Self::MetaParseAt => "_meta_parse_at",
            Self::MetaIsExpr => "_meta_isexpr",
            Self::MetaQuot => "_meta_quot",
            Self::MetaIsIdentifier => "Meta.isidentifier",
            Self::MetaIsOperator => "Meta.isoperator",
            Self::MetaIsUnaryOperator => "Meta.isunaryoperator",
            Self::MetaIsBinaryOperator => "Meta.isbinaryoperator",
            Self::MetaIsPostfixOperator => "Meta.ispostfixoperator",
            Self::MetaLower => "_meta_lower",
            Self::MacroExpand => "macroexpand",
            Self::MacroExpandBang => "macroexpand!",
            Self::IncludeString => "include_string",
            Self::EvalFile => "evalfile",

            // Higher-Order Functions
            Self::Compose => "compose",

            // Test Operations
            Self::TestRecord => "_test_record!",
            Self::TestRecordBroken => "_test_record_broken!",
            Self::TestSetBegin => "_testset_begin!",
            Self::TestSetEnd => "_testset_end!",

            // Regex Operations
            Self::RegexNew => "Regex",
            Self::RegexMatch => "match",
            Self::RegexOccursin => "occursin",
            Self::RegexReplace => "_regex_replace",
            Self::RegexSplit => "split",
            Self::RegexEachmatch => "eachmatch",
        }
    }

    /// Check if this builtin is a pure math function (no side effects).
    pub fn is_pure_math(&self) -> bool {
        matches!(
            self,
            // Note: Sin, Cos, Tan, Asin, Acos, Atan, Exp, Log removed — now Pure Julia (base/math.jl)
            Self::Floor
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
                | Self::TruncSigDigits
                | Self::NextFloat
                | Self::PrevFloat // Note: Gcd, Lcm, Factorial removed - now Pure Julia (base/intfuncs.jl)
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
                | Self::Sort
                | Self::Print
                | Self::Println
                | Self::Sleep
                | Self::DictSet
                | Self::DictDelete
                | Self::DictGetBang
                | Self::DictMergeBang
                | Self::DictEmpty
                | Self::SetPush
                | Self::SetDelete
                | Self::SetEmpty
                | Self::_SetPush
                | Self::_SetDelete
                | Self::_SetEmpty
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
