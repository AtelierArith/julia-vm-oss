# Test function definitions inside @testset (Issue #902)
# Functions defined inside macro blocks should be callable within the same scope

using Test

# Test basic function definition inside @testset
@testset "function inside testset" begin
    function f(x)
        2x
    end
    @test f(2) == 4
    @test f(5) == 10
end

# Test short function definition inside @testset
@testset "short function inside testset" begin
    g(x) = x * x
    @test g(3) == 9
    @test g(4) == 16
end

# Test multiple functions inside @testset
@testset "multiple functions inside testset" begin
    function add(a, b)
        a + b
    end
    function mul(a, b)
        a * b
    end
    @test add(2, 3) == 5
    @test mul(2, 3) == 6
end

# Test function with typed parameters inside @testset
@testset "typed function inside testset" begin
    function typed_add(x::Int, y::Int)
        x + y
    end
    @test typed_add(10, 20) == 30
end

true
