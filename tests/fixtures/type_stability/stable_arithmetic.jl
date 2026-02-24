# Type-stable arithmetic functions

using Test

# Type-stable function: Int64 -> Int64
function double_int(x::Int64)
    return x * 2
end

# Type-stable function: Float64 -> Float64
function double_float(x::Float64)
    return x * 2.0
end

# Type-stable function: Float64, Float64 -> Float64
function add_floats(a::Float64, b::Float64)
    return a + b
end

@testset "Type-stable arithmetic functions" begin
    @test double_int(5) == 10
    @test double_float(2.5) == 5.0
    @test add_floats(1.0, 2.0) == 3.0
end

true
