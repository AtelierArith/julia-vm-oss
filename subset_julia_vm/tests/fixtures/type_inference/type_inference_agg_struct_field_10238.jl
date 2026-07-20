# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: type_inference/parametric_struct_distinct_instantiations.jl =====
module Agg_parametric_struct_distinct_instantiations
# Parametric struct constructor inference must not pick an arbitrary instantiation
# Issue #3534: HashMap iteration order can return the wrong type_id for parametric structs
# when multiple instantiations of the same base struct exist.

using Test

struct Box3534{T}
    value::T
end

function f3534()
    a = Box3534{Int64}(1)
    b = Box3534{String}("x")
    return (a.value, b.value)
end

@testset "Parametric struct distinct instantiations" begin
    result = f3534()
    @test result == (1, "x")
    @test result[1] == 1
    @test result[2] == "x"
end
end # module Agg_parametric_struct_distinct_instantiations

# ===== source: type_inference/struct_field_inference.jl =====
module Agg_struct_field_inference
# Struct field type inference test
# Tests that field access on user-defined structs correctly infers field types

using Test

# Define test structs OUTSIDE @testset block per project guidelines
struct Point
    x::Float64
    y::Float64
end

struct Person
    name::String
    age::Int64
end

struct Container{T}
    value::T
end

mutable struct MutablePoint
    x::Float64
    y::Float64
end

@testset "User-defined struct field access type inference" begin
    # Test basic struct field access
    p = Point(1.0, 2.0)
    @test p.x == 1.0
    @test p.y == 2.0

    # Test different field types
    person = Person("Alice", 30)
    @test person.name == "Alice"
    @test person.age == 30

    # Test field access in arithmetic expressions
    p2 = Point(3.0, 4.0)
    dist_squared = p.x * p.x + p.y * p.y
    @test dist_squared == 5.0

    # Test field access in function calls
    function point_magnitude(pt)
        return sqrt(pt.x * pt.x + pt.y * pt.y)
    end
    @test point_magnitude(p2) == 5.0

    # Test mutable struct field access
    mp = MutablePoint(1.0, 2.0)
    @test mp.x == 1.0
    mp.x = 10.0
    @test mp.x == 10.0

    # Test nested field access
    function swap_xy(pt::Point)
        return Point(pt.y, pt.x)
    end
    swapped = swap_xy(Point(3.0, 4.0))
    @test swapped.x == 4.0
    @test swapped.y == 3.0

    # Test field access in loops
    points = [Point(1.0, 1.0), Point(2.0, 2.0), Point(3.0, 3.0)]
    sum_x = 0.0
    for pt in points
        sum_x += pt.x
    end
    @test sum_x == 6.0

    # Test parametric struct field access
    c_int = Container{Int64}(42)
    @test c_int.value == 42

    c_float = Container{Float64}(3.14)
    @test c_float.value == 3.14
end
end # module Agg_struct_field_inference

# ===== source: type_inference/ternary_struct_vs_primitive.jl =====
module Agg_ternary_struct_vs_primitive
# Ternary inference must not silently drop a non-struct branch when the other is a struct.
# Issue #3533

using Test

struct S3533
    x::Int64
end

function f3533(c)
    v = c ? S3533(1) : 0
    return v
end

function g3533(c)
    v = c ? S3533(2) : nothing
    return v
end

@testset "Ternary struct vs primitive branches" begin
    @test f3533(true) isa S3533
    @test f3533(true).x == 1
    @test f3533(false) == 0

    @test g3533(true) isa S3533
    @test g3533(false) === nothing
end
end # module Agg_ternary_struct_vs_primitive

true
