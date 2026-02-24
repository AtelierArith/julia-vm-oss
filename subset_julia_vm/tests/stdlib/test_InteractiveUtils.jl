# Test for InteractiveUtils-like functionality
# Note: The InteractiveUtils module is not yet fully functional in SubsetJuliaVM.
# This tests built-in type introspection functions that are available.
using Test

@testset "typeof builtin" begin
    # typeof returns type name as string in SubsetJuliaVM
    # Just verify it runs without error
    t1 = typeof(1)
    t2 = typeof(1.0)
    t3 = typeof(true)
    t4 = typeof("hello")
    t5 = typeof([1, 2, 3])
    t6 = typeof(nothing)

    # Use length to verify strings are returned (non-empty)
    @test length(t1) > 0
    @test length(t2) > 0
    @test length(t3) > 0
    @test length(t4) > 0
    @test length(t5) > 0
    @test length(t6) > 0
end

@testset "isa builtin" begin
    # isa checks if a value is of a certain type
    # Note: Type must be passed as string in SubsetJuliaVM
    # isa returns 1 for true, 0 for false
    # Workaround: assign to variable first due to parser limitation with @test isa(...)
    r1 = isa(1, "Int64")
    @test r1 == 1
    r2 = isa(1, "Float64")
    @test r2 == 0
    r3 = isa(1.0, "Float64")
    @test r3 == 1
    r4 = isa(1.0, "Int64")
    @test r4 == 0
    # Note: In SubsetJuliaVM, true is stored as Int64
    r5 = isa(true, "Int64")
    @test r5 == 1
    r6 = isa("hello", "String")
    @test r6 == 1
    # Note: Arrays are stored as Vector{Float64} in SubsetJuliaVM
    r7 = isa([1, 2], "Vector{Float64}")
    @test r7 == 1
end

println("test_InteractiveUtils.jl: All tests passed!")
