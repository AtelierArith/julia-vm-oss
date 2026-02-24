# Test flatten/isflat loop fusion infrastructure (Issue #2544)
# Verifies that nested Broadcasted objects are flattened into single-level
# structures with fused functions for efficient loop execution.
# Reference: julia/base/broadcast.jl L324-407

using Test

# double: simple test function
function double(x)
    return x * 2
end

# add3: ternary function for testing
function add3(x, y, z)
    return x + y + z
end

@testset "isflat" begin
    x = [1.0, 2.0, 3.0]

    # Flat Broadcasted (no nested Broadcasted in args)
    bc_flat = Broadcasted(sin, (x,))
    @test isflat(bc_flat) == true

    # Binary flat Broadcasted
    y = [4.0, 5.0, 6.0]
    bc_flat2 = Broadcasted(+, (x, y))
    @test isflat(bc_flat2) == true

    # Nested Broadcasted → not flat
    bc_inner = Broadcasted(cos, (x,))
    bc_nested = Broadcasted(sin, (bc_inner,))
    @test isflat(bc_nested) == false

    # Mixed: one Broadcasted + one leaf → not flat
    bc_mixed = Broadcasted(+, (bc_inner, y))
    @test isflat(bc_mixed) == false
end

@testset "cat_nested" begin
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]
    z = [7.0, 8.0, 9.0]

    # Single nested: Broadcasted(sin, (Broadcasted(cos, (x,)),))
    bc_inner = Broadcasted(cos, (x,))
    bc_outer = Broadcasted(sin, (bc_inner,))
    leaves = cat_nested(bc_outer)
    @test length(leaves) == 1

    # Nested with extra leaf: Broadcasted(+, (Broadcasted(cos, (x,)), y))
    bc_mixed = Broadcasted(+, (bc_inner, y))
    leaves2 = cat_nested(bc_mixed)
    @test length(leaves2) == 2

    # Both nested: Broadcasted(+, (Broadcasted(sin, (x,)), Broadcasted(cos, (y,))))
    bc_sin = Broadcasted(sin, (x,))
    bc_cos = Broadcasted(cos, (y,))
    bc_both = Broadcasted(+, (bc_sin, bc_cos))
    leaves3 = cat_nested(bc_both)
    @test length(leaves3) == 2

    # Deep nesting: Broadcasted(sin, (Broadcasted(cos, (Broadcasted(double, (x,)),)),))
    bc_double = Broadcasted(double, (x,))
    bc_cos2 = Broadcasted(cos, (bc_double,))
    bc_deep = Broadcasted(sin, (bc_cos2,))
    leaves4 = cat_nested(bc_deep)
    @test length(leaves4) == 1

    # Binary inner: Broadcasted(sin, (Broadcasted(+, (x, y)),))
    bc_add = Broadcasted(+, (x, y))
    bc_sin_add = Broadcasted(sin, (bc_add,))
    leaves5 = cat_nested(bc_sin_add)
    @test length(leaves5) == 2
end

@testset "make_makeargs" begin
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]

    # Single leaf: selector returns the first element
    selectors1 = make_makeargs((x,))
    @test length(selectors1) == 1
    @test selectors1[1]((10.0, 20.0)) == 10.0

    # Two leaves: selectors pick first and second
    selectors2 = make_makeargs((x, y))
    @test length(selectors2) == 2
    @test selectors2[1]((10.0, 20.0)) == 10.0
    @test selectors2[2]((10.0, 20.0)) == 20.0

    # One Broadcasted arg: selector applies inner function
    bc_inner = Broadcasted(cos, (x,))
    selectors3 = make_makeargs((bc_inner,))
    @test length(selectors3) == 1
    # The selector should apply cos to the first flat arg
    @test selectors3[1]((0.0,)) == cos(0.0)
    @test selectors3[1]((1.0,)) == cos(1.0)
end

@testset "flatten basic" begin
    x = [1.0, 2.0, 3.0]

    # Already flat → returns same object
    bc_flat = Broadcasted(sin, (x,))
    bc_still_flat = flatten(bc_flat)
    @test isflat(bc_still_flat) == true

    # sin(cos(x)) → fused single-level
    bc_cos = Broadcasted(cos, (x,))
    bc_nested = Broadcasted(sin, (bc_cos,))
    bc_fused = flatten(bc_nested)
    @test isflat(bc_fused) == true
    @test length(bc_fused.bc_args) == 1

    # Verify the fused function computes sin(cos(x))
    result_nested = copy(bc_nested)
    result_fused = copy(bc_fused)
    @test length(result_nested) == 3
    @test length(result_fused) == 3
    # sin(cos(1.0)), sin(cos(2.0)), sin(cos(3.0))
    @test result_nested[1] == result_fused[1]
    @test result_nested[2] == result_fused[2]
    @test result_nested[3] == result_fused[3]
end

@testset "flatten binary outer with one nested arg" begin
    x = [1.0, 2.0, 3.0]
    y = [10.0, 20.0, 30.0]

    # +(cos(x), y) → fused 2-arg function
    bc_cos = Broadcasted(cos, (x,))
    bc_add = Broadcasted(+, (bc_cos, y))
    @test isflat(bc_add) == false
    bc_fused = flatten(bc_add)
    @test isflat(bc_fused) == true
    @test length(bc_fused.bc_args) == 2

    result_nested = copy(bc_add)
    result_fused = copy(bc_fused)
    for i in 1:3
        # cos(x[i]) + y[i]
        @test result_nested[i] == result_fused[i]
    end

    # +(x, cos(y)) → fused 2-arg function (second arg nested)
    bc_cos2 = Broadcasted(cos, (y,))
    bc_add2 = Broadcasted(+, (x, bc_cos2))
    bc_fused2 = flatten(bc_add2)
    @test isflat(bc_fused2) == true
    @test length(bc_fused2.bc_args) == 2

    result_nested2 = copy(bc_add2)
    result_fused2 = copy(bc_fused2)
    for i in 1:3
        @test result_nested2[i] == result_fused2[i]
    end
end

@testset "flatten binary outer with both nested" begin
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]

    # +(sin(x), cos(y)) → fused 2-arg
    bc_sin = Broadcasted(sin, (x,))
    bc_cos = Broadcasted(cos, (y,))
    bc_add = Broadcasted(+, (bc_sin, bc_cos))
    @test isflat(bc_add) == false
    bc_fused = flatten(bc_add)
    @test isflat(bc_fused) == true
    @test length(bc_fused.bc_args) == 2

    result_nested = copy(bc_add)
    result_fused = copy(bc_fused)
    for i in 1:3
        # sin(x[i]) + cos(y[i])
        @test result_nested[i] == result_fused[i]
    end
end

@testset "flatten deep nesting (3 levels)" begin
    x = [0.5, 1.0, 1.5]

    # sin(cos(double(x))) — 3 levels deep
    bc_double = Broadcasted(double, (x,))
    bc_cos = Broadcasted(cos, (bc_double,))
    bc_sin = Broadcasted(sin, (bc_cos,))
    @test isflat(bc_sin) == false
    bc_fused = flatten(bc_sin)
    @test isflat(bc_fused) == true
    @test length(bc_fused.bc_args) == 1

    result_nested = copy(bc_sin)
    result_fused = copy(bc_fused)
    for i in 1:3
        # sin(cos(double(x[i]))) = sin(cos(x[i]*2))
        @test result_nested[i] == result_fused[i]
    end
end

@testset "flatten with binary inner" begin
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]

    # sin(x + y) → sin(+(x, y))
    bc_add = Broadcasted(+, (x, y))
    bc_sin = Broadcasted(sin, (bc_add,))
    @test isflat(bc_sin) == false
    bc_fused = flatten(bc_sin)
    @test isflat(bc_fused) == true
    @test length(bc_fused.bc_args) == 2

    result_nested = copy(bc_sin)
    result_fused = copy(bc_fused)
    for i in 1:3
        # sin(x[i] + y[i])
        @test result_nested[i] == result_fused[i]
    end
end

@testset "flatten with scalar args" begin
    x = [1.0, 2.0, 3.0]

    # +(cos(x), 10.0) — scalar second arg
    bc_cos = Broadcasted(cos, (x,))
    bc_add = Broadcasted(+, (bc_cos, 10.0))
    bc_fused = flatten(bc_add)
    @test isflat(bc_fused) == true

    result_nested = copy(bc_add)
    result_fused = copy(bc_fused)
    for i in 1:3
        @test result_nested[i] == result_fused[i]
    end
end

true
