# Issue #4824 (prevention): typed-vector / typed-comprehension intercept cluster.
#
# Covers the cluster of bugs #4811 / #4816 / #4818 / #4819 / #4822, all of which
# shared one root: compile-time *intercepts* in
# `compile/expr/collection.rs` (`compile_array_constructor`,
# `compile_comprehension`) that short-circuit method dispatch with hardcoded
# happy-path assumptions and previously produced a wrong-typed result when the
# assumption failed. Each cluster bug has its own per-issue regression fixture;
# this is the *combinatorial parity probe* asked for in #4824 — a single sweep
# over (target T) x (argument shape) that locks in the correct value AND
# `typeof` so a future change to the intercept path cannot silently break any
# cell without tripping this probe.
#
# Every assertion below was verified to match upstream Julia 1.12 for both the
# resulting value and `typeof`. Coverage matrix:
#   target T   : Int64, Int8, Float64, Float32, Any, String, Char, Symbol
#   arg shapes : UnitRange, StepRange, StepRangeLen (int+float step),
#                Vector{S} (S in {Int,Float64,String,Char,Symbol}),
#                empty array, plus T[expr for x in iter] typed comprehension
#                and the plain Any-body comprehension fallback (#4822).
#
# NOTE ON SCOPE: the #4824 cluster of *fixed* bugs is strictly the numeric and
# Any element types (Int*/Float*/Any) over range/array/empty shapes. While
# probing for this fixture, separate, out-of-cluster divergences were found for
# Bool/Char/Symbol/String *typed comprehensions* (`Bool[...]`, `Char[...]`,
# `Symbol[...]`, `String[...]`) and for `Vector{T}(::Tuple)`; those are NOT part
# of the #4811/#4816/#4818/#4819/#4822 cluster and are tracked as their own
# issues. They are deliberately excluded here so this probe stays focused on the
# cluster it guards.
#
# UPDATE: the Bool/Char/Symbol/String typed-comprehension divergence (#5040) has
# since been fixed and is covered by its own dedicated regression fixture
# `array/typed_comprehension_nonnumeric_eltypes_5040.jl` (single- and
# multi-iterator, filter, empty iterator, and Int->Char convert cells). It is
# kept separate from this cluster probe by design. `Vector{T}(::Tuple)` (#5041)
# remains out of scope.

using Test

# ---- #4811: Vector{T}(::AbstractRange) typed range constructor ----
@testset "Vector{T}(range): UnitRange Int -> Float64 (#4811)" begin
    v = Vector{Float64}(1:3)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end
@testset "Vector{T}(range): UnitRange Int -> Int64 identity (#4811)" begin
    v = Vector{Int64}(1:3)
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2, 3]
end
@testset "Vector{T}(range): UnitRange Int -> Int8 (#4811)" begin
    v = Vector{Int8}(1:3)
    @test typeof(v) === Vector{Int8}
    @test v == Int8[1, 2, 3]
end
@testset "Vector{T}(range): UnitRange Int -> Float32 (#4811)" begin
    v = Vector{Float32}(1:3)
    @test typeof(v) === Vector{Float32}
    @test v == Float32[1.0, 2.0, 3.0]
end
@testset "Vector{T}(range): StepRange Int -> Float64 (#4811)" begin
    v = Vector{Float64}(1:2:9)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 3.0, 5.0, 7.0, 9.0]
end
@testset "Vector{T}(range): StepRangeLen Float -> Float64 (#4811)" begin
    v = Vector{Float64}(1.0:0.5:3.0)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 1.5, 2.0, 2.5, 3.0]
end
@testset "Vector{T}(range): Float range -> Int64 (#4811)" begin
    v = Vector{Int64}(1.0:3.0)
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2, 3]
end
@testset "Vector{T}(range): UnitRange -> Any boxes (#4818/#4811)" begin
    v = Vector{Any}(1:3)
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test v == [1, 2, 3]
end

# ---- #4816: Vector{T}(::Vector{S}) eltype conversion ----
@testset "Vector{T}(arr): Int -> Float64 (#4816)" begin
    v = Vector{Float64}([1, 2, 3])
    @test typeof(v) === Vector{Float64}
    @test eltype(v) === Float64
    @test v == [1.0, 2.0, 3.0]
end
@testset "Vector{T}(arr): Float -> Int64 (#4816)" begin
    v = Vector{Int64}([1.0, 2.0])
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2]
end
@testset "Vector{T}(arr): Float64 -> Float32 (#4816)" begin
    v = Vector{Float32}([1.0, 2.0])
    @test typeof(v) === Vector{Float32}
    @test v == Float32[1.0, 2.0]
end
@testset "Vector{T}(arr): Int -> Int8 (#4816)" begin
    v = Vector{Int8}([1, 2, 3])
    @test typeof(v) === Vector{Int8}
    @test v == Int8[1, 2, 3]
end
@testset "Vector{T}(arr): same eltype Int64 fast path (#4816)" begin
    v = Vector{Int64}([1, 2, 3])
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2, 3]
end
@testset "Vector{T}(arr): same eltype Float64 fast path (#4816)" begin
    v = Vector{Float64}([1.0, 2.0])
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0]
end

# ---- #4818: Vector{Any}(::Vector{S}) boxing ----
@testset "Vector{Any}(arr): Int -> Any (#4818)" begin
    v = Vector{Any}([1, 2, 3])
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test v == [1, 2, 3]
end
@testset "Vector{Any}(arr): Float -> Any (#4818)" begin
    v = Vector{Any}([1.0, 2.0, 3.0])
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test v == [1.0, 2.0, 3.0]
end

# ---- non-numeric eltype constructor path (verified matching on main) ----
@testset "Vector{String}(arr) identity (#4816 path)" begin
    v = Vector{String}(["a", "b"])
    @test typeof(v) === Vector{String}
    @test v == ["a", "b"]
end
@testset "Vector{Char}(arr) identity (#4816 path)" begin
    v = Vector{Char}(['a', 'b'])
    @test typeof(v) === Vector{Char}
    @test v == ['a', 'b']
end
@testset "Vector{Symbol}(arr) identity (#4816 path)" begin
    v = Vector{Symbol}([:a, :b])
    @test typeof(v) === Vector{Symbol}
    @test v == [:a, :b]
end

# ---- empty array argument shape ----
@testset "Vector{Float64}(empty Int[]) (#4816)" begin
    v = Vector{Float64}(Int[])
    @test typeof(v) === Vector{Float64}
    @test length(v) == 0
end
@testset "Vector{Any}(empty Int[]) (#4818)" begin
    v = Vector{Any}(Int[])
    @test typeof(v) === Vector{Any}
    @test length(v) == 0
end

# ---- #4819: Any[expr for x in iter] typed-Any comprehension ----
@testset "Any[x for x in array] (#4819)" begin
    v = Any[x for x in [1, 2, 3]]
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test v == [1, 2, 3]
end
@testset "Any[x for x in range] (#4819)" begin
    v = Any[x for x in 1:3]
    @test typeof(v) === Vector{Any}
    @test v == [1, 2, 3]
end
@testset "Any[x*2 for x in array] non-identity body (#4819)" begin
    v = Any[x * 2 for x in [1, 2, 3]]
    @test typeof(v) === Vector{Any}
    @test v == [2, 4, 6]
end
@testset "Any[x for x in empty range] (#4819)" begin
    v = Any[x for x in 1:0]
    @test typeof(v) === Vector{Any}
    @test length(v) == 0
end

# ---- typed comprehension for concrete numeric T (intercept path) ----
@testset "Float64[x for x in range] (#4819 regression guard)" begin
    v = Float64[x for x in 1:3]
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end
@testset "Float64[x for x in array] (#4816/#4819)" begin
    v = Float64[x for x in [1, 2, 3]]
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end
@testset "Int8[x for x in range] (#4819)" begin
    v = Int8[x for x in 1:3]
    @test typeof(v) === Vector{Int8}
    @test v == Int8[1, 2, 3]
end

# ---- #4822: Any-body comprehension must not silently coerce to Float64 ----
@testset "[convert(Any,x) for x in Int array] no Float coercion (#4822)" begin
    v = [convert(Any, x) for x in [1, 2, 3]]
    # Upstream infers Vector{Int64}; sjulia preserves losslessly as Vector{Any}.
    # Both are acceptable per #4822 — what is NOT acceptable is Vector{Float64}
    # with silent Float coercion. Assert values and forbid the F64 coercion.
    @test eltype(v) !== Float64
    @test v[1] === 1
    @test v[2] === 2
    @test v[3] === 3
    @test v == [1, 2, 3]
end
@testset "[convert(Any,x) for x in String array] no Float coercion (#4822)" begin
    v = [convert(Any, x) for x in ["a", "b"]]
    @test eltype(v) !== Float64
    @test v == ["a", "b"]
end

# ---- plain comprehension regressions (intercept inference) ----
@testset "untyped Int comprehension stays Vector{Int64} (#4822)" begin
    v = [x for x in [1, 2, 3]]
    @test typeof(v) === Vector{Int64}
end
@testset "untyped Float comprehension stays Vector{Float64} (#4822)" begin
    v = [Float64(x) for x in [1, 2, 3]]
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end

true
