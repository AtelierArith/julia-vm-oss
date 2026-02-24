# Tests for VectorOf type tracking through reassignment (Issue #2319)
# julia_type_locals should track VectorOf types from variable reassignment

using Test

# Target functions with parametric vector dispatch
function vec_sum(v::Vector{Int64})
    return sum(v)
end

function vec_product(v::Vector{Float64})
    s = 1.0
    for x in v
        s = s * x
    end
    return s
end

# Variable reassignment preserves Int vector type
v1 = [1, 2, 3, 4]
v2 = v1

# Variable reassignment preserves Float vector type
f1 = [2.0, 3.0, 4.0]
f2 = f1

# Chain reassignment
v3 = v2

@testset "VectorOf type preservation through reassignment (Issue #2319)" begin
    # Direct literal (baseline)
    @test vec_sum([1, 2, 3, 4]) == 10
    @test vec_product([2.0, 3.0, 4.0]) == 24.0

    # Variable reassignment preserves type
    @test vec_sum(v2) == 10
    @test vec_product(f2) == 24.0

    # Chain reassignment (v3 = v2 = v1)
    @test vec_sum(v3) == 10
end

# Vector from conditional expression
v4 = if true
    [10, 20, 30]
else
    [40, 50, 60]
end

@testset "VectorOf from conditional expression (Issue #2319)" begin
    @test vec_sum(v4) == 60
end

true
