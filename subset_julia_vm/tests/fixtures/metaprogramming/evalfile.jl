# Test evalfile function - evaluate code from file
# Note: SubsetJuliaVM's eval only supports limited expressions:
#       - Arithmetic: +, -, *, /, div, mod, ^
#       - Math functions: sqrt, abs, sin, cos
#       - Comparisons: ==, !=, <, >, <=, >=
#       - Boolean: !

using Test

@testset "evalfile basic" begin
    # Use the static test file in the same directory
    # evalfile_test_file.jl contains: 1 + 2 + 3 + 4 (which equals 10)
    # The tests are run from subset_julia_vm directory

    result = evalfile("tests/fixtures/metaprogramming/evalfile_test_file.jl")
    @test result == 10
end

true
