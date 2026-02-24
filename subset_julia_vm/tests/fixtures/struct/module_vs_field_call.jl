# Test that module calls and field function calls are correctly distinguished
# This tests the is_known_module_name() function in lowering
# Related: Issue #1360

using Test

# Define functions to store in fields
square(x) = x * x
cube(x) = x * x * x

# Struct with function field
struct FuncHolder
    f::Function
end

@testset "Module vs field call distinction" begin
    # Field function calls: lowercase variable name should be treated as variable
    holder = FuncHolder(square)
    @test holder.f(3) == 9
    @test holder.f(4) == 16

    # Different function in field
    holder2 = FuncHolder(cube)
    @test holder2.f(2) == 8
    @test holder2.f(3) == 27

    # Module calls should still work: Base.abs etc.
    @test Base.abs(-5) == 5
    @test Base.abs(-3.14) > 3.0

    # Module calls with math functions (using 2-arg versions)
    @test Base.min(3, 1) == 1
    @test Base.max(1, 4) == 4
end

true
