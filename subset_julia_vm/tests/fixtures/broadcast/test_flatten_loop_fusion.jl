# Test broadcast loop fusion end-to-end via dot expressions (Issue #2679)
# Verifies that flatten() correctly fuses nested dot operations
# into efficient single-pass computations through the copyto! pipeline.
# These tests exercise the full pipeline: lowering → Broadcasted → flatten → copyto!

using Test

# Helper function for testing
function double(x)
    return x * 2
end

@testset "nested dot expressions: sin.(x) .+ cos.(x)" begin
    x = [0.0, 1.0, 2.0]
    result = sin.(x) .+ cos.(x)
    @test length(result) == 3
    @test result[1] == sin(0.0) + cos(0.0)
    @test result[2] == sin(1.0) + cos(1.0)
    @test result[3] == sin(2.0) + cos(2.0)
end

@testset "triple nesting: abs.(sin.(x))" begin
    x = [0.5, 1.0, 1.5]
    result = abs.(sin.(x))
    @test length(result) == 3
    @test result[1] == abs(sin(0.5))
    @test result[2] == abs(sin(1.0))
    @test result[3] == abs(sin(1.5))
end

@testset "mixed scalar/array: sin.(x) .+ 1" begin
    x = [0.0, 1.0, 2.0]
    result = sin.(x) .+ 1
    @test length(result) == 3
    @test result[1] == sin(0.0) + 1
    @test result[2] == sin(1.0) + 1
    @test result[3] == sin(2.0) + 1
end

@testset "chained binary: (a .+ b) .* c" begin
    a = [1.0, 2.0, 3.0]
    b = [4.0, 5.0, 6.0]
    c = [10.0, 20.0, 30.0]
    result = (a .+ b) .* c
    @test result[1] == (1.0 + 4.0) * 10.0
    @test result[2] == (2.0 + 5.0) * 20.0
    @test result[3] == (3.0 + 6.0) * 30.0
end

@testset "deep chain: double.(sin.(cos.(x)))" begin
    x = [0.5, 1.0, 1.5]
    result = double.(sin.(cos.(x)))
    @test length(result) == 3
    @test result[1] == double(sin(cos(0.5)))
    @test result[2] == double(sin(cos(1.0)))
    @test result[3] == double(sin(cos(1.5)))
end

@testset "scalar .+ nested: 10 .+ cos.(x)" begin
    x = [0.0, 1.0, 2.0]
    result = 10 .+ cos.(x)
    @test length(result) == 3
    @test result[1] == 10 + cos(0.0)
    @test result[2] == 10 + cos(1.0)
    @test result[3] == 10 + cos(2.0)
end

true
