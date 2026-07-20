# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: reflection/array_dimensional_alias_5593.jl =====

@testset "Array dimensional aliases in runtime type objects (Issue #5593)" begin
    @test string(Base.unwrap_unionall(Array)) == "Array{T, N}"
    @test string(Base.unwrap_unionall(Vector)) == "Array{T, 1}"
    @test string(Base.unwrap_unionall(Matrix)) == "Array{T, 2}"
    @test string(Base.unwrap_unionall(DenseArray)) == "DenseArray{T, N}"
    @test string(Base.unwrap_unionall(DenseVector)) == "DenseArray{T, 1}"
    @test string(Base.unwrap_unionall(DenseMatrix)) == "DenseArray{T, 2}"

    @test Base.unwrap_unionall(Vector).parameters[2] === 1
    @test Base.unwrap_unionall(Matrix).parameters[2] === 2
    @test Base.unwrap_unionall(DenseVector).parameters[2] === 1
    @test Base.unwrap_unionall(DenseMatrix).parameters[2] === 2

    @test typeof(DenseArray) === UnionAll
    @test typeof(DenseVector) === UnionAll
    @test nameof(Vector) === :Array
    @test nameof(DenseVector) === :DenseArray

    @test supertype(Vector{Int}) === DenseVector{Int64}
    @test supertype(Matrix{Int}) === DenseMatrix{Int64}
    @test supertype(Array{Int,3}) === DenseArray{Int64,3}
    @test supertype(DenseVector{Int}) === AbstractVector{Int64}

    @test Vector{Int} <: DenseVector{Int}
    @test DenseVector{Int} === DenseArray{Int,1}
    @test DenseMatrix{Float64} === DenseArray{Float64,2}

    @test Base.rewrap_unionall(Base.unwrap_unionall(Vector), Vector) === Vector
    @test Base.rewrap_unionall(Base.unwrap_unionall(Array), Array) === Array
    @test Base.rewrap_unionall(Base.unwrap_unionall(DenseVector), DenseVector) === DenseVector
    @test Base.rewrap_unionall(Base.unwrap_unionall(DenseArray), DenseArray) === DenseArray
end

# ===== source: reflection/base_function_value_general_8137.jl =====
# Ordinary Base functions resolve as callable function values via qualified
# `Base.<name>` access (Issue #8137). Continuation of the Base.<fn> value-lookup
# series (#4960-#4966 / umbrella #4119): the earlier work covered specific
# reflection/conversion helpers; this covers the general case — now-Pure-Julia
# Base functions (`map`, `filter`, `sin`, `cos`, `reduce`, `foldl`, `sum`, …)
# that are not in the `is_base_function` allowlist but ARE backed by a method
# table, so `f = Base.map` must produce the same callable as the unqualified
# `map`. Upstream Julia resolves `Base.map` to the function object. The `map`
# assertions also pin Issue #9981: function-value dispatch must select eager
# `Base.map(f, A)`, not the lazy `Iterators.map`/`Generator` shim.

@testset "Base higher-order function values" begin
    f = Base.map
    @test f isa Function
    @test f(x -> x + 1, [1, 2, 3]) == [2, 3, 4]

    g = Base.filter
    @test g isa Function
    @test g(iseven, [1, 2, 3, 4]) == [2, 4]

    r = Base.reduce
    @test r isa Function
    @test r(+, [1, 2, 3, 4]) == 10

    fl = Base.foldl
    @test fl isa Function
    @test fl(+, [1, 2, 3, 4]) == 10

    s = Base.sum
    @test s isa Function
    @test s([1, 2, 3]) == 6
end

@testset "Base math function values" begin
    sn = Base.sin
    @test sn isa Function
    @test sn(0.0) == 0.0

    cs = Base.cos
    @test cs isa Function
    @test cs(0.0) == 1.0
end

@testset "qualified Base.<fn> ignores a same-named local shadow" begin
    # Qualified `Base.map` must resolve to the Base function regardless of a
    # same-named local binding — qualified access bypasses local shadowing.
    map = 99
    h = Base.map
    @test h isa Function
    @test h(x -> x * 2, [1, 2, 3]) == [2, 4, 6]
    @test map == 99
end

# ===== source: reflection/base_function_value_lookup_4960.jl =====
# Base.<fn> resolves as a callable function value (Issues #4960-#4966)
# Upstream Julia exposes these Base helpers as function values that can be
# bound to a variable and applied.

@testset "Base reflection function values" begin
    rt = Base.return_types
    @test rt isa Function
    @test rt(+, Tuple{Int64,Int64}) == [Int64]

    ct = Base.code_typed
    @test ct isa Function

    cl = Base.code_lowered
    @test cl isa Function

    irt = Base.infer_return_type
    @test irt isa Function
end

@testset "Base conversion/promotion function values" begin
    w = Base.widen
    @test w isa Function
    @test w(Int32) == Int64

    pt = Base.promote_type
    @test pt isa Function
    @test pt(Int64, Float64) == Float64

    pr = Base.promote_rule
    @test pr isa Function

    cv = Base.convert
    @test cv isa Function
    @test cv(Float64, 3) == 3.0

    ot = Base.oftype
    @test ot isa Function
    @test ot(1.0, 3) == 3.0
end

# ===== source: reflection/collect_simplevector_svec_5196.jl =====
# Issue #5196: collect over a heterogeneous Core.SimpleVector (svec).
#
# `collect(<Type>.parameters)` / `collect(Core.svec(...))` must materialize a
# `Vector{Any}` preserving heterogeneous elements (types and/or values), exactly
# like upstream Julia. Previously sjulia inferred a numeric element type and
# tried to coerce type-object elements, failing with
# "expected numeric value, got DataType". Unlike a Tuple, `collect` over an svec
# always yields `Vector{Any}` (never narrows to a concrete element type).


@testset "collect over Core.SimpleVector (svec) -> Vector{Any} (Issue #5196)" begin
    # --- Heterogeneous type-parameter svecs via <Type>.parameters ---
    a = collect(Tuple{Int,String}.parameters)
    @test a == Any[Int, String]
    @test typeof(a) === Vector{Any}
    @test a[1] === Int
    @test a[2] === String

    b = collect(Dict{String,Int}.parameters)
    @test b == Any[String, Int]
    @test typeof(b) === Vector{Any}

    # Mixed type + integer value parameter (Array dimensionality N).
    c = collect(Vector{Int}.parameters)
    @test c == Any[Int, 1]
    @test typeof(c) === Vector{Any}
    @test c[1] === Int
    @test c[2] === 1

    # --- Core.svec(...) constructor (mixed value + type elements) ---
    d = collect(Core.svec(1, "a", Int))
    @test d == Any[1, "a", Int]
    @test typeof(d) === Vector{Any}

    # --- Homogeneous numeric svec still collects to Vector{Any} (NOT Vector{Int}) ---
    e = collect(Core.svec(1, 2, 3))
    @test e == Any[1, 2, 3]
    @test typeof(e) === Vector{Any}
    @test eltype(e) === Any

    # --- Homogeneous type svec ---
    f = collect(Core.svec(Int, String))
    @test f == Any[Int, String]
    @test typeof(f) === Vector{Any}

    # --- Empty svec ---
    g = collect(Core.svec())
    @test typeof(g) === Vector{Any}
    @test length(g) == 0

    # --- eltype of an svec is always Any (both constructor and .parameters forms) ---
    @test eltype(Core.svec(Int, String)) === Any
    @test eltype(Tuple{Int,String}.parameters) === Any
    @test eltype(Core.svec(1, 2, 3)) === Any

    # --- setindex! of type objects into a Vector{Any} (the root cause) ---
    h = Vector{Any}(undef, 2)
    h[1] = Int
    h[2] = String
    @test h == Any[Int, String]
end

# ===== source: reflection/core_typeofbottom_module_lookup_9967.jl =====
# Issue #9967: direct `Core.<name>` module-qualified lookup for `TypeofBottom`.
# Upstream `julia` resolves `Core` as a real module and exposes `TypeofBottom`
# as the singleton type of the bottom type object `Union{}`
# (`typeof(Union{}) === Core.TypeofBottom`). sjulia previously failed to
# resolve `Core` at all in source code ("Unknown module: Core") because the
# qualified-name path only special-cased a fixed set of `Core.<name>` bindings
# and had no general fallback for `Core`. `Core.TypeofBottom` is now resolved
# through the same structural module-lookup mechanism already used for
# `Core.SimpleVector` / `Core.Builtin` (Issue #4722 / #5129).


@testset "Core.TypeofBottom module lookup (Issue #9967)" begin
    # Bare `Core.TypeofBottom` resolves and displays like upstream.
    @test string(Core.TypeofBottom) == "Core.TypeofBottom"
    @test typeof(Core.TypeofBottom) == DataType

    # Identity with `typeof(Union{})` (the bottom type object).
    @test Core.TypeofBottom === typeof(Union{})
    @test typeof(Union{}) === Core.TypeofBottom

    # Another already-modeled `Core.<name>` binding resolves through the same
    # mechanism (Issue #4722), confirming the fix is structural, not a
    # TypeofBottom-only special case.
    @test string(Core.SimpleVector) == "Core.SimpleVector"
    @test typeof(Core.SimpleVector) == DataType
end

# ===== source: reflection/dense_array_supertype_3909.jl =====

@testset "Dense array supertype aliases (Issue #3909)" begin
    @test string(DenseArray) == "DenseArray"
    @test string(DenseVector) == "DenseVector"
    @test string(DenseMatrix) == "DenseMatrix"
    @test string(supertype(Vector{Int})) == "DenseVector{Int64}"
    @test string(supertype(Matrix{Int})) == "DenseMatrix{Int64}"
    @test string(supertype(Array{Int,1})) == "DenseVector{Int64}"
    @test string(supertype(Array{Int,2})) == "DenseMatrix{Int64}"
    @test string(supertype(Array{Int,3})) == "DenseArray{Int64, 3}"
    @test string(supertype(DenseVector{Int})) == "AbstractVector{Int64}"
    @test string(supertype(DenseMatrix{Int})) == "AbstractMatrix{Int64}"
    @test string(supertype(DenseArray{Int,1})) == "AbstractVector{Int64}"
    @test string(supertype(DenseArray{Int,2})) == "AbstractMatrix{Int64}"
    @test string(supertype(DenseArray{Int,3})) == "AbstractArray{Int64, 3}"
    @test Vector{Int} <: DenseVector{Int}
    @test Matrix{Int} <: DenseMatrix{Int}
    @test Array{Int,2} <: DenseMatrix{Int}
    @test Array{Int,3} <: DenseArray{Int,3}
    @test !(Vector{Int} <: DenseVector{Float64})
    @test !(Array{Int,2} <: DenseArray{Int,1})
end

# ===== source: reflection/expr_getfield_through_any_7614.jl =====
# Issue #7614: `getfield`/`getproperty` on an `Expr` value must resolve the
# `head`/`args` fields, matching upstream Julia.
#
# The `.head`/`.args` property syntax is compile-time special-cased to a
# dedicated `GetExprField` instruction, but explicit `getfield(ex, :head)` and
# `getproperty(ex, :head)` calls (which a macro helper hits when the receiver is
# carried through an `Any`-typed parameter, e.g. `MacroTools.splitdef`) routed
# to the generic reflection `getfield`, which rejected `Expr`.


@testset "getfield/getproperty on Expr (Issue #7614)" begin
    ex = :(x + 1)

    # Field access by Symbol name.
    @test getfield(ex, :head) == :call
    @test getfield(ex, :args) == Any[:+, :x, 1]

    # Field access by 1-based integer index (1 => head, 2 => args).
    @test getfield(ex, 1) == :call
    @test getfield(ex, 2) == Any[:+, :x, 1]

    # getproperty mirrors getfield for the default (no custom getproperty) case.
    @test getproperty(ex, :head) == :call
    @test getproperty(ex, :args) == Any[:+, :x, 1]

    # The same access through an `Any`-typed parameter (the actual failing
    # scenario inside MacroTools helpers).
    head_of(e) = getfield(e, :head)
    args_of(e) = getfield(e, :args)
    @test head_of(ex) == :call
    @test args_of(ex) == Any[:+, :x, 1]

    # `args` returns the shared backing array (reference identity), so mutating
    # it through `getfield` updates the owning Expr — matching upstream's
    # `args::Array{Any,1}` reference semantics.
    @test getfield(ex, :args) === ex.args
    @test getfield(ex, 2) === ex.args
end

# ===== source: reflection/fresh_typevar_identity.jl =====

@testset "fresh TypeVar identity" begin
    a = TypeVar(:T)
    b = TypeVar(:T)

    @test a.name === :T
    @test a.lb === Union{}
    @test a.ub === Any

    @test a === a
    @test !(a === b)
    @test objectid(a) != objectid(b)
    @test isequal(a, a)
    @test !isequal(a, b)

    bounded = TypeVar(:S, Union{}, Integer)
    @test bounded.name === :S
    @test bounded.lb === Union{}
    @test bounded.ub === Integer
end

# ===== source: reflection/getfield_module_binding_4621.jl =====

@testset "getfield(Main, Symbol) resolves function bindings (#4621)" begin
    fname = :reduce
    f = getfield(Main, fname)
    @test f == reduce
    @test typeof(f) == typeof(reduce)

    for name in (:reduce, :foldl, :foldr)
        resolved = getfield(Main, name)
        @test typeof(resolved) <: Function
    end
end

# ===== source: reflection/hasfield_builtin_type_objects.jl =====
# Test hasfield for builtin runtime type objects


@testset "hasfield - builtin runtime type objects" begin
    @test hasfield(LineNumberNode, :line)
    @test hasfield(LineNumberNode, :file)
    @test !hasfield(LineNumberNode, :missing)

    @test hasfield(Expr, :head)
    @test hasfield(Expr, :args)
    @test !hasfield(Expr, :missing)

    @test hasfield(QuoteNode, :value)
    @test !hasfield(QuoteNode, :args)

    @test hasfield(GlobalRef, :mod)
    @test hasfield(GlobalRef, :name)
    @test hasfield(GlobalRef, :binding)
    @test !hasfield(GlobalRef, :missing)
end

# ===== source: reflection/isvarargtype_4701.jl =====

@testset "Base.isvarargtype recognises Vararg-bearing DataTypes (Issue #4701)" begin
    @test Base.isvarargtype(Vararg{Int})
    @test Base.isvarargtype(Vararg{Float64})
    @test !Base.isvarargtype(Int)
    @test !Base.isvarargtype(Float64)
    @test !Base.isvarargtype(Tuple{Int, Int})
    @test !Base.isvarargtype(Vector{Int})
end

@testset "Base.isvatuple detects trailing Vararg in Tuple types (Issue #4701)" begin
    @test Base.isvatuple(Tuple{Int, Vararg{Int}})
    @test Base.isvatuple(Tuple{Vararg{Int}})
    @test Base.isvatuple(Tuple{Int, String, Vararg{Any}})
    @test !Base.isvatuple(Tuple{Int, Int})
    @test !Base.isvatuple(Tuple{})
    @test !Base.isvatuple(Tuple{Int, String})
end

# ===== source: reflection/module_equality_4959.jl =====
# Module equality compares module identity (Issue #4959)
# Upstream Julia: Base == Base => true, Base == Core => false

@testset "module equality" begin
    @test Base == Base
    @test Core == Core
    @test !(Base == Core)
    @test Base != Core
    @test Base === Base
    @test !(Base === Core)
    @test Base !== Core
end

# ===== source: reflection/parameters_simplevector_svec_4722.jl =====
# Issue #4722: <DataType>.parameters returns a Core.SimpleVector (svec),
# not a Tuple. Covers svec display, typeof / isa identity, length, indexing,
# iteration, structural ===, == and the Core.svec(...) constructor.


@testset "Core.SimpleVector (svec) parity for <DataType>.parameters" begin
    # .parameters of a concrete Tuple type is an svec of element types
    p = typeof((1, 2.0, "x")).parameters

    # Display: svec(...)
    @test string(p) == "svec(Int64, Float64, String)"

    # Type identity: typeof(p) === Core.SimpleVector and isa
    @test typeof(p) === Core.SimpleVector
    @test p isa Core.SimpleVector
    @test isa(p, Core.SimpleVector)

    # length
    @test length(p) == 3

    # 1-based indexing returns the element types
    @test p[1] === Int64
    @test p[2] === Float64
    @test p[3] === String

    # iteration yields the elements in order
    collected = []
    for x in p
        push!(collected, x)
    end
    @test length(collected) == 3
    @test collected[1] === Int64
    @test collected[3] === String

    # Dict parameters: svec of (K, V)
    d = Dict{String, Int64}.parameters
    @test string(d) == "svec(String, Int64)"
    @test d[1] === String
    @test d[2] === Int64

    # Structural === (svec has by-content identity in Julia)
    @test (Dict{String, Int64}.parameters === Dict{String, Int64}.parameters)
    @test !(Dict{String, Int64}.parameters === Dict{Int64, String}.parameters)
    @test (typeof((1, 2)).parameters === typeof((3, 4)).parameters)
    @test !(typeof((1, 2)).parameters === typeof((1, 2, 3)).parameters)

    # == compares by content
    @test (typeof((1, 2)).parameters == typeof((3, 4)).parameters)

    # Core.svec(...) constructor (value parameters can be non-type values)
    s = Core.svec(Int64, 2, :sym)
    @test string(s) == "svec(Int64, 2, :sym)"
    @test typeof(s) === Core.SimpleVector
    @test s isa Core.SimpleVector
    @test length(s) == 3
    @test s[2] == 2
    @test (s === Core.svec(Int64, 2, :sym))
    @test (s == Core.svec(Int64, 2, :sym))

    # Empty svec
    e = Core.svec()
    @test string(e) == "svec()"
    @test length(e) == 0
    @test isempty(e)

    # Splat: svec expands like a tuple
    countargs(args...) = length(args)
    @test countargs(p...) == 3

    # The type itself prints fully qualified
    @test string(Core.SimpleVector) == "Core.SimpleVector"
end

# ===== source: reflection/parameters_value_params_5162.jl =====
# Issue #5162: `<Type>.parameters` includes integer/value parameters, not just
# type parameters. Upstream Julia: `Array{T,N}.parameters == svec(T, N)`,
# `Vector{Int}.parameters == svec(Int64, 1)`, `Val{5}.parameters == svec(5)`.
# Follow-up to #4722/#5161 (svec identity for `.parameters`).
# Verified against upstream Julia 1.12 (parity).


@testset "value/integer parameters in <Type>.parameters (Issue #5162)" begin
    # --- Array dimensionality value parameter N -------------------------------
    @test Vector{Int}.parameters == Core.svec(Int64, 1)
    @test Matrix{Float64}.parameters == Core.svec(Float64, 2)
    @test Array{Int,3}.parameters == Core.svec(Int64, 3)
    @test typeof([1]).parameters == Core.svec(Int64, 1)
    @test typeof([1.0 2.0; 3.0 4.0]).parameters == Core.svec(Float64, 2)

    # The value parameter is the integer itself (an Int64), not a type.
    @test Vector{Int}.parameters[2] === 1
    @test Matrix{Float64}.parameters[2] === 2
    @test Array{Int,3}.parameters[2] === 3
    @test typeof(Vector{Int}.parameters[2]) === Int64

    # Type parameter still comes first.
    @test Vector{Int}.parameters[1] === Int64
    @test Matrix{Float64}.parameters[1] === Float64

    # Length now counts the value parameter.
    @test length(Vector{Int}.parameters) == 2
    @test length(Matrix{Float64}.parameters) == 2
    @test length(Array{Int,3}.parameters) == 2

    # --- Val: a pure value parameter ------------------------------------------
    @test Val{5}.parameters == Core.svec(5)
    @test Val{:foo}.parameters == Core.svec(:foo)
    @test Val{true}.parameters == Core.svec(true)
    @test typeof(Val(7)).parameters == Core.svec(7)
    @test Val{5}.parameters[1] === 5
    @test typeof(Val{5}.parameters[1]) === Int64
    @test Val{:foo}.parameters[1] === :foo
    @test Val{true}.parameters[1] === true
    @test length(Val{5}.parameters) == 1

    # --- Type-only parameters unaffected (no regression of #5161 svec) --------
    @test Tuple{Int,String}.parameters == Core.svec(Int64, String)
    @test NTuple{3,Int}.parameters == Core.svec(Int64, Int64, Int64)
    @test Dict{String,Int}.parameters == Core.svec(String, Int64)

    # --- Result identity / type stays Core.SimpleVector (svec) ----------------
    @test typeof(Vector{Int}.parameters) === Core.SimpleVector
    @test typeof(Val{5}.parameters) === Core.SimpleVector
    @test isa(Array{Int,3}.parameters, Core.SimpleVector)
    @test isa(Val{:foo}.parameters, Core.SimpleVector)

    # --- Splat of a value-parameter-bearing svec ------------------------------
    pair(a, b) = (a, b)
    @test pair(Vector{Int}.parameters...) === (Int64, 1)
    @test pair(Array{Int,3}.parameters...) === (Int64, 3)

    # --- getfield form matches dot form ---------------------------------------
    @test getfield(Vector{Int}, :parameters) == Core.svec(Int64, 1)
    @test getfield(Matrix{Float64}, :parameters) == Core.svec(Float64, 2)

    # --- Display form: svec(...) with value parameters ------------------------
    @test string(Vector{Int}.parameters) == "svec(Int64, 1)"
    @test string(Array{Int,3}.parameters) == "svec(Int64, 3)"
    @test string(Val{:foo}.parameters) == "svec(:foo)"
    @test string(Val{true}.parameters) == "svec(true)"

    # --- Dynamic field-access path (t typed as Any) yields value params too ---
    getparams(t) = t.parameters
    @test getparams(Vector{Int}) == Core.svec(Int64, 1)
    @test typeof(getparams(Vector{Int})) === Core.SimpleVector
end

# ===== source: reflection/parametric_typevar_identity_4698.jl =====

# Issue #4698: a fresh TypeVar embedded in a parametric type's `.parameters`
# must remain `===` to the original TypeVar object, not just name-equal.
@testset "parametric TypeVar identity (Issue #4698)" begin
    T = TypeVar(:T)

    # Vector{T}.parameters[1] is the *same* TypeVar object as T.
    @test Vector{T}.parameters[1] === T
    @test Matrix{T}.parameters[1] === T

    # Distinct TypeVars keep distinct identity.
    S = TypeVar(:S)
    @test Vector{S}.parameters[1] === S
    @test !(Vector{S}.parameters[1] === T)
    @test !(Vector{T}.parameters[1] === S)

    # Reconstructing the same parametric type recovers the same TypeVar.
    @test Vector{T}.parameters[1] === Vector{T}.parameters[1]

    # The recovered parameter is a TypeVar, isa Any, and prints like upstream.
    p = Vector{T}.parameters[1]
    @test p isa TypeVar
    @test isequal(p, T)
end

# ===== source: reflection/reflection_pair_fieldtypes_5733.jl =====

# Issue #5733: fieldtypes/fieldtype on a Pair{A,B} type returned (Any, Any) — sjulia
# represents Pair as a non-parametric struct, so its declared field types are
# untyped. They are now resolved from the type arguments: first::A, second::B.

@testset "fieldtypes/fieldtype on Pair{A,B} (Issue #5733)" begin
    @test fieldtypes(Pair{Int,String}) == (Int64, String)
    @test fieldtype(Pair{Int,String}, 1) == Int64
    @test fieldtype(Pair{Int,String}, 2) == String
    @test fieldtype(Pair{Int,String}, :first) == Int64
    @test fieldtype(Pair{Int,String}, :second) == String
    @test fieldtypes(Pair{Float64,Int}) == (Float64, Int64)
    @test fieldtypes(Pair{String,Vector{Int}}) == (String, Vector{Int64})
    @test fieldtypes(Pair{Symbol,Int}) == (Symbol, Int64)

    # Bare Pair (no parameters) is unchanged.
    @test fieldtypes(Pair) == (Any, Any)

    # User parametric structs and Complex are unaffected (regression).
    @test fieldtypes(Complex{Float64}) == (Float64, Float64)

    # fieldnames still consistent.
    @test fieldnames(Pair{Int,String}) == (:first, :second)
end

# ===== source: reflection/runtime_type_object_identity.jl =====

@testset "runtime type object identity projections" begin
    @test objectid(Int64) == objectid(Int64)
    @test objectid(Int64) != objectid(Float64)
    @test objectid(Vector) == objectid(Vector)
    @test objectid(Vector{Int64}) != objectid(Vector{Float64})

    vector_t = Vector.var
    @test vector_t === Vector.body.parameters[1]

    dict_k = Dict.var
    dict_v = Dict.body.var
    dict_params = Dict.body.body.parameters
    @test dict_params[1] === dict_k
    @test dict_params[2] === dict_v

    @test fieldtypes(GlobalRef)[1] === Module
    @test fieldtypes(GlobalRef)[2] === Symbol
end

# ===== source: reflection/type_objectid_hash_dict_key_5108.jl =====
# Type objects as Dict/Set keys with stable, equality-consistent
# objectid / hash (Issue #5108).
#
# Upstream `objectid`/`hash` integers are NOT portable across versions or
# sessions, so this fixture asserts the OBSERVABLE CONTRACT (which holds
# identically under upstream Julia 1.12), never literal hash values:
#   - equal types hash/objectid equal; distinct types (almost surely) do not
#   - type objects are usable as Dict keys and Set elements
#   - a type key is distinct from instances of that type


@testset "type objects: hash / objectid consistency (Issue #5108)" begin
    # Equal types hash equal (concrete, parametric, abstract, Union)
    @test hash(Int) === hash(Int)
    @test hash(Float64) === hash(Float64)
    @test hash(Vector{Int}) === hash(Vector{Int})
    @test hash(Pair{Int,String}) === hash(Pair{Int,String})
    @test hash(Number) === hash(Number)

    # Distinct types (almost surely) hash differently
    @test hash(Int) != hash(Float64)
    @test hash(Vector{Int}) != hash(Vector{Float64})
    @test hash(Pair{Int,String}) != hash(Pair{String,Int})

    # objectid mirrors the hash contract
    @test objectid(Int) === objectid(Int)
    @test objectid(Vector{Int}) === objectid(Vector{Int})
    @test objectid(Int) != objectid(Float64)
end

@testset "type objects as Dict keys (Issue #5108)" begin
    # Dict{Type,Int}: insert, lookup, overwrite
    d = Dict{Type,Int}()
    d[Int] = 1
    d[Float64] = 2
    @test d[Int] === 1
    @test d[Float64] === 2
    d[Int] = 10
    @test d[Int] === 10
    @test length(d) == 2

    # Dict literal with type keys
    @test Dict(Int => 1)[Int] === 1
    d2 = Dict(Int => 1, Float64 => 2, String => 3)
    @test d2[String] === 3
    @test length(d2) == 3

    # Parametric / abstract / Union type keys
    dp = Dict{Type,Int}()
    dp[Pair{Int,String}] = 7
    @test dp[Pair{Int,String}] === 7
    @test haskey(dp, Pair{Int,String})
    @test !haskey(dp, Pair{String,Int})

    du = Dict{Type,Int}()
    du[Number] = 1
    du[Union{Int,String}] = 2
    @test du[Number] === 1
    @test du[Union{Int,String}] === 2
    @test haskey(du, Union{Int,String})

    # get (read-only) and delete!
    dg = Dict{Type,Int}(Int => 5, Float64 => 9)
    @test get(dg, Int, -1) === 5
    @test get(dg, String, -1) === -1
    delete!(dg, Int)
    @test !haskey(dg, Int)
    @test length(dg) == 1

    # keys() round-trips the type objects back to usable values
    dk = Dict{Type,Int}(Int => 1, Float64 => 2)
    ks = collect(keys(dk))
    @test Int in ks
    @test Float64 in ks
end

@testset "type objects as Set elements (Issue #5108)" begin
    s = Set{Type}([Int, Float64, Int])
    @test length(s) == 2
    @test Int in s
    @test Float64 in s
    @test !(String in s)
end

@testset "type key distinct from its instances (Issue #5108)" begin
    da = Dict{Any,Int}()
    da[Int] = 100   # the type object as a key
    da[1] = 200     # an instance of that type as a key
    @test da[Int] === 100
    @test da[1] === 200
    @test haskey(da, Int)
    @test length(da) == 2
end

# ===== source: reflection/typevar_unionall_reflection.jl =====

@testset "TypeVar and UnionAll reflection" begin
    tv = Vector.var
    @test tv.name === :T
    @test tv.lb === Union{}
    @test tv.ub === Any

    vector_body = Vector.body
    vector_body_params = vector_body.parameters
    @test vector_body_params[1] === tv

    dict_k = Dict.var
    dict_body = Dict.body
    dict_v = dict_body.var
    dict_concrete_body = dict_body.body
    dict_params = dict_concrete_body.parameters

    @test dict_k.name === :K
    @test dict_v.name === :V
    @test dict_params[1] === dict_k
    @test dict_params[2] === dict_v
end

@testset "eltype for reflected type parameters" begin
    @test eltype(Array) === Any
    @test eltype(Vector) === Any
    @test eltype(Vector{Int64}) === Int64
    @test eltype(Matrix{Bool}) === Bool
end

# ===== source: reflection/unionall_constructor_4694.jl =====

@testset "UnionAll(var, body) constructor (Issue #4694)" begin
    T = TypeVar(:T)

    # When the body does not reference the bound variable, return the body
    # unchanged (matches upstream `jl_type_unionall`).
    @test UnionAll(T, Int64) === Int64
    @test UnionAll(T, Vector{Int64}) === Vector{Int64}
    @test !isa(UnionAll(T, Vector{Int64}), UnionAll)

    # Bounded TypeVars are accepted as the var argument
    S = TypeVar(:S, Union{}, Integer)
    @test UnionAll(S, Float64) === Float64
end

@testset "Base.rewrap_unionall round-trips Base.unwrap_unionall (Issue #4694)" begin
    # Substituting a concrete type strips all UnionAll layers, because the
    # concrete type does not reference the bound variables.
    @test Base.rewrap_unionall(Int64, Int64) === Int64
    @test Base.rewrap_unionall(Int64, Vector) === Int64
    @test Base.rewrap_unionall(Int64, Dict) === Int64

    # Re-wrapping the unwrapped body restores a UnionAll. The nested Dict
    # case stays a UnionAll (the body still has `K` and `V` references).
    @test isa(Base.rewrap_unionall(Base.unwrap_unionall(Vector), Vector), UnionAll)
    @test isa(Base.rewrap_unionall(Base.unwrap_unionall(Dict), Dict), UnionAll)
end

# ===== source: reflection/unwrap_rewrap_unionall_roundtrip_5105.jl =====

@testset "unwrap_unionall on concrete types is a no-op (Issue #5105)" begin
    @test Base.unwrap_unionall(Int64) === Int64
    @test Base.unwrap_unionall(Float64) === Float64
    @test Base.unwrap_unionall(Vector{Int64}) === Vector{Int64}
    @test Base.unwrap_unionall(Dict{Symbol,Int64}) === Dict{Symbol,Int64}
    @test !isa(Base.unwrap_unionall(Vector{Int64}), UnionAll)
end

@testset "unwrap_unionall strips outer UnionAll wrappers (Issue #5105)" begin
    @test isa(Vector, UnionAll)
    @test !isa(Base.unwrap_unionall(Vector), UnionAll)
    @test !isa(Base.unwrap_unionall(Dict), UnionAll)
    @test !isa(Base.unwrap_unionall(Set), UnionAll)
end

@testset "rewrap_unionall round-trips unwrap_unionall (Issue #5105)" begin
    @test Base.rewrap_unionall(Base.unwrap_unionall(Vector), Vector) === Vector
    @test Base.rewrap_unionall(Base.unwrap_unionall(Set), Set) === Set
    @test Base.rewrap_unionall(Base.unwrap_unionall(Dict), Dict) === Dict
    # rewrap onto a non-UnionAll returns the body unchanged
    @test Base.rewrap_unionall(Int64, Int64) === Int64
    @test Base.rewrap_unionall(Int64, Vector) === Int64
    # round-trip result is again a UnionAll
    @test isa(Base.rewrap_unionall(Base.unwrap_unionall(Vector), Vector), UnionAll)
end

true
