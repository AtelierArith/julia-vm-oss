# applicable(f, args...) public reflection lookup (Issue #4957)
using Test

f_appl_4957(x::Int) = x
g_appl_4957(x::Int, y::Int) = x + y

@testset "applicable lookup" begin
    # Built-in operator
    @test applicable(+, 1, 2) == true
    @test applicable(+, 1, "a") == false

    # User single-method functions
    @test applicable(f_appl_4957, 1) == true
    @test applicable(f_appl_4957, "a") == false
    @test applicable(f_appl_4957) == false

    # Two-argument user function
    @test applicable(g_appl_4957, 1, 2) == true
    @test applicable(g_appl_4957, 1, "a") == false
    @test applicable(g_appl_4957, 1) == false

    # Return type is Bool
    @test applicable(+, 1, 2) isa Bool
end

true
