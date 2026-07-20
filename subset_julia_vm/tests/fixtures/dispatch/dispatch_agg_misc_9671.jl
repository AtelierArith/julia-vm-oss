# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: dispatch/abstract_dict_array_disambiguation_4708.jl =====

@testset "dispatcher disambiguates Array from AbstractString / AbstractDict (Issue #4708)" begin
    # Before #4708, defining all three overloads triggered an
    # `AmbiguousMethod` compile error for `myfn([1, 2, 3])` because the
    # JuliaType::AbstractUser parent-`Any` fallback in is_subtype_of
    # spuriously made Array <: AbstractString and Array <: AbstractDict.
    myfn(s::AbstractString) = "string"
    myfn(d::AbstractDict)   = "dict"
    myfn(a::AbstractArray)  = "array"
    myfn(x)                  = "any"

    @test myfn([1, 2, 3]) == "array"
    @test myfn("hello") == "string"
    @test myfn(Dict("a" => 1)) == "dict"
    @test myfn(42) == "any"
    @test myfn(nothing) == "any"
end

@testset "Dict <: AbstractDict and Set <: AbstractSet stay true (Issue #4708)" begin
    # Verify the CoreType-backed fallback for the `Any`-parent case
    # preserves the built-in container hierarchy.
    @test Dict{String, Int}() isa AbstractDict
    @test Set{Int}() isa AbstractSet
    # And the dispatch agrees:
    container_kind(::AbstractDict) = :dict
    container_kind(::AbstractSet)  = :set
    container_kind(::AbstractArray) = :array
    @test container_kind(Dict("a" => 1)) === :dict
    @test container_kind(Set([1, 2])) === :set
    @test container_kind([1, 2, 3]) === :array
end

# ===== source: dispatch/complex_array_isa_logical_element_3908.jl =====

@testset "complex array isa uses logical element type (Issue #3908)" begin
    zeros_complex = zeros(Complex{Float64}, 2)
    erased = Any[zeros_complex]

    @test isa(zeros_complex, Vector{Complex{Float64}})
    @test isa(erased[1], Vector{Complex{Float64}})
    @test !isa(zeros_complex, Vector{Float64})
end

# ===== source: dispatch/dispatch_abstractmatrix_no_loose_match_function_7334.jl =====

# Issue #7334: a `::AbstractMatrix` (= `AbstractArray{T,2}`) method parameter must
# only match 2-dimensional array values. Before the fix, the compile-time
# struct-parents fallback conservatively accepted a *function singleton*
# (`typeof(sin)`, a `Function`) as a subtype of `AbstractMatrix` — so `h(sin)`
# loose-matched `h(::AbstractMatrix)` and even won dispatch over the specific
# `h(::Function)` method (the same conservative-accept class as #7266). Upstream
# Julia selects `h(::Function)` for `h(sin)` and raises a `MethodError` when only
# an `::AbstractMatrix` method exists. This blocked the #7275 Interact sample
# (`scatter(sin)` crashed inside `scatter(::AbstractMatrix)`).
@testset "Issue #7334: ::AbstractMatrix does not loose-match a Function" begin
    h(m::AbstractMatrix) = "matrix"
    h(x::Function) = "function"
    h(x::Int) = "int"
    h(x::AbstractString) = "string"

    # A Function argument must reach the specific `::Function` method, not
    # `::AbstractMatrix`.
    @test h(sin) == "function"
    @test h(cos) == "function"

    # The other specific methods are unaffected.
    @test h(3) == "int"
    @test h("hi") == "string"

    # A genuine 2-D array still reaches `::AbstractMatrix`.
    @test h([1.0 2.0; 3.0 4.0]) == "matrix"

    # With only an `::AbstractMatrix` method (no `::Function` competitor), a
    # Function argument has NO matching method and must raise a MethodError —
    # the conservative accept must not silently route it into the matrix method.
    g(m::AbstractMatrix) = "g-matrix"
    g(x::Int) = "g-int"
    @test_throws MethodError g(sin)
    @test g([1 2; 3 4]) == "g-matrix"
    @test g(5) == "g-int"
end

# ===== source: dispatch/float16_mixed_type_dispatch.jl =====
# Test Float16 mixed-type arithmetic dispatch
# Issue #1898: F16+I64, F16+F32, F16+F64 dispatch paths were missing


@testset "Float16 mixed-type arithmetic" begin
    # F16 + I64 -> F16
    @test Float16(2.5) + 1 == Float16(3.5)
    @test typeof(Float16(2.5) + 1) == Float16
    @test 1 + Float16(2.5) == Float16(3.5)
    @test typeof(1 + Float16(2.5)) == Float16

    # F16 - I64 -> F16
    @test Float16(5.0) - 2 == Float16(3.0)
    @test typeof(Float16(5.0) - 2) == Float16

    # F16 * I64 -> F16
    @test Float16(2.0) * 3 == Float16(6.0)
    @test typeof(Float16(2.0) * 3) == Float16

    # F16 / I64 -> F16
    @test Float16(6.0) / 2 == Float16(3.0)
    @test typeof(Float16(6.0) / 2) == Float16

    # F16 comparison with I64
    @test Float16(2.5) > 2
    @test Float16(2.5) < 3
    @test Float16(2.0) == 2
    @test Float16(2.5) != 2
    @test Float16(2.0) >= 2
    @test Float16(2.0) <= 2
end

@testset "Float16-Float64 promotion" begin
    # F16 + F64 -> F64
    @test Float16(2.5) + 1.0 == 3.5
    @test typeof(Float16(2.5) + 1.0) == Float64

    # F16 - F64 -> F64
    @test Float16(5.0) - 2.0 == 3.0
    @test typeof(Float16(5.0) - 2.0) == Float64

    # F16 * F64 -> F64
    @test Float16(2.0) * 3.0 == 6.0
    @test typeof(Float16(2.0) * 3.0) == Float64

    # F16 / F64 -> F64
    @test Float16(6.0) / 2.0 == 3.0
    @test typeof(Float16(6.0) / 2.0) == Float64
end

@testset "Float16-Float32 promotion" begin
    # F16 + F32 -> F32
    @test Float16(2.5) + Float32(1.0) == Float32(3.5)
    @test typeof(Float16(2.5) + Float32(1.0)) == Float32

    # F16 * F32 -> F32
    @test Float16(2.0) * Float32(3.0) == Float32(6.0)
    @test typeof(Float16(2.0) * Float32(3.0)) == Float32
end

# ===== source: dispatch/module_param_specificity_5005.jl =====
# Issue #5005: ::Module parameter must win dispatch specificity over an
# untyped parameter when both methods match.


@testset "::Module wins specificity over untyped parameter (Issue #5005)" begin
    foo(m::Module, s::Symbol) = "module-form"
    foo(x, s::Symbol) = "generic-form"
    @test foo(Base, :sum) == "module-form"
    @test foo(Core, :Int) == "module-form"
    @test foo(Main, :x) == "module-form"
    @test foo(42, :y) == "generic-form"   # untyped method still reachable

    # Reverse declaration order must not change the winner.
    bar(x, s::Symbol) = "generic-bar"
    bar(m::Module, s::Symbol) = "module-bar"
    @test bar(Base, :sum) == "module-bar"
    @test bar("str", :y) == "generic-bar"

    # Module beats Any in a single-argument shape too.
    baz(m::Module) = "module-baz"
    baz(x) = "generic-baz"
    @test baz(Base) == "module-baz"
    @test baz(3.0) == "generic-baz"

    # Module's runtime type is exactly Module.
    @test typeof(Base) === Module
    @test isa(Base, Module)
end

# ===== source: dispatch/nothing_missing_singleton_5069.jl =====
# Issue #5069: systematic type-system integration of the singleton types
# Nothing (`nothing`) and Missing (`missing`).
#
# Covers: singleton type identity, isa/subtype, Union{T,Nothing} (the Optional
# pattern) runtime dispatch f(::Nothing) vs f(::Int), the isnothing/ismissing
# predicates, something/coalesce, and basic Missing propagation. Matches upstream
# Julia exactly. Verified against `julia` before landing.


@testset "Nothing/Missing singleton identity" begin
    @test typeof(nothing) === Nothing
    @test typeof(missing) === Missing
    @test isa(nothing, Nothing)
    @test isa(missing, Missing)
    @test !isa(nothing, Missing)
    @test !isa(missing, Nothing)
end

@testset "Nothing/Missing subtype (post Issue #5257)" begin
    # Nothing/Missing are concrete singleton DataTypes, NOT the bottom type.
    @test !(Nothing <: Int64)
    @test !(Missing <: Int64)
    @test Nothing <: Any
    @test Missing <: Any
    @test Nothing <: Nothing
    @test Missing <: Missing
    @test Nothing <: Union{Int64,Nothing}
    @test Missing <: Union{Int64,Missing}
    @test !(Nothing <: Union{Int64,Float64})
end

@testset "isa with Union (Optional pattern membership)" begin
    @test nothing isa Union{Nothing,Int}
    @test missing isa Union{Missing,Int}
    @test !(nothing isa Union{Missing,Int})
    @test !(missing isa Union{Nothing,Int})
end

@testset "Runtime Union dispatch f(::Nothing) vs f(::Int)" begin
    f(::Nothing) = "got nothing"
    f(::Int) = "got int"
    @test f(nothing) == "got nothing"
    @test f(3) == "got int"

    # via a wrapper so the argument flows as a value, not a literal
    relay(x) = f(x)
    @test relay(nothing) == "got nothing"
    @test relay(5) == "got int"
end

@testset "Runtime Union dispatch g(::Missing) vs g(::Int)" begin
    g(::Missing) = "got missing"
    g(::Int) = "got int"
    @test g(missing) == "got missing"
    @test g(9) == "got int"
end

@testset "Optional pattern: Union{Int,Nothing} parameter" begin
    function opt(x::Union{Int,Nothing})
        if isnothing(x)
            return -1
        else
            return x + 100
        end
    end
    @test opt(nothing) == -1
    @test opt(5) == 105
end

@testset "isnothing / ismissing predicates" begin
    @test isnothing(nothing)
    @test !isnothing(1)
    @test !isnothing(missing)
    @test ismissing(missing)
    @test !ismissing(1)
    @test !ismissing(nothing)
end

@testset "something / coalesce narrowing" begin
    @test something(nothing, 1) == 1
    @test something(nothing, nothing, 3) == 3
    @test something(Some(7), 1) == 7
    # `missing` is a real value: something stops at the first non-nothing
    @test ismissing(something(missing, nothing, 3))

    @test coalesce(missing, 2) == 2
    @test coalesce(missing, missing, 7) == 7
    @test coalesce(1, 2) == 1
    @test ismissing(coalesce(missing, missing))
end

@testset "Missing propagation basics" begin
    @test ismissing(missing + 1)
    @test ismissing(1 + missing)
    @test ismissing(missing - missing)
    @test ismissing(missing * 2)
    @test ismissing(missing == 1)
    @test ismissing(missing < 2)
    @test (missing + 1) === missing
    # === is identity (Bool), not three-valued
    @test (missing === missing) == true
    @test (missing === 1) == false
    @test (nothing === nothing) == true
end

# ===== source: dispatch/typed_dispatch_bigint_concrete_guard_9768.jl =====
# Runtime typed dispatch must not route BigInt into an Int64-only method.


@testset "typed dispatch BigInt concrete guard (Issue #9768)" begin
    route9768(x::Int64) = :int64
    route9768(x::Integer) = :integer

    @test route9768(BigInt(3)) == :integer
    @test route9768(Int64(3)) == :int64
end

# ===== source: dispatch/uint_int_mixed_comparison.jl =====
# UInt8/Int64 mixed-type comparison and arithmetic (Issue #1853)
# Tests that UInt types can be compared and promoted with Int types


@testset "UInt8 mixed-type comparison" begin
    # UInt8 == Int64
    x = UInt8(72)
    @test x == 72
    @test 72 == x

    # UInt8 != Int64
    @test x != 73
    @test 73 != x

    # UInt8 < Int64
    @test x < 100
    @test !(x < 72)

    # UInt8 > Int64
    @test x > 50
    @test !(x > 72)

    # UInt8 <= Int64
    @test x <= 72
    @test x <= 100

    # UInt8 >= Int64
    @test x >= 72
    @test x >= 50
end

@testset "UInt8 promotion rules" begin
    # promote_type with explicit Type arguments
    @test promote_type(Int64, UInt8) == Int64
    @test promote_type(UInt8, Int64) == Int64
    @test promote_type(Int64, UInt16) == Int64
    @test promote_type(Int64, UInt32) == Int64
    @test promote_type(Int128, UInt64) == Int128

    # promote_rule direct
    @test promote_rule(Int64, UInt8) == Int64
    @test promote_rule(Int32, UInt8) == Int32
    @test promote_rule(Int16, UInt8) == Int16
end

@testset "UInt8 arithmetic with Int64" begin
    x = UInt8(10)
    y = 20

    # Addition
    @test x + y == 30

    # Subtraction
    @test y - x == 10

    # Multiplication
    @test x * y == 200
end

# ===== source: dispatch/where_bounds_upstream_applicability_8427.jl =====

@testset "where bounds match upstream applicability (Issue #8427)" begin
    lower_value(x::T) where {T>:Int} = 1
    @test lower_value(1.0) == 1
    @test lower_value(1) == 1

    lower_type(x::T) where {T>:Int} = T
    @test_throws UndefVarError lower_type(1.0)

    cross_value(x::T, y::S) where {T<:Real,S<:T} = 1
    @test cross_value(1, 2.0) == 1
    @test cross_value(1, 1) == 1

    cross_type(x::T, y::S) where {T<:Real,S<:T} = (T, S)
    @test cross_type(1, 1) == (Int64, Int64)
    @test_throws UndefVarError cross_type(1, 2.0)
end

true
