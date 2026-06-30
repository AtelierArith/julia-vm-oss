# GlobalRef callable tests
# GlobalRef can be called as a function: ref(args...) calls the referenced function
# Issue #302: Full GlobalRef support
#
# NOTE: This feature extends Julia's GlobalRef to be callable, which is NOT
# standard Julia behavior. This test will fail in official Julia.
# In SubsetJuliaVM, GlobalRef(mod, :name)(args...) resolves and calls the function.

using Test

# Define test functions in Main scope
function add_numbers(a, b)
    return a + b
end

function square(x)
    return x * x
end

function globalref_dispatch_3910(x)
    return :any
end

function globalref_dispatch_3910(x::Int64)
    return :int
end

struct GlobalRefAbsBox3910
    x::Int64
end

Base.abs(x::GlobalRefAbsBox3910) = :box_abs
Base.length(x::Symbol) = 3910

@testset "GlobalRef callable: calling user-defined functions" begin
    # Test 1: Call function with multiple arguments
    # Note: In official Julia, GlobalRef takes a Module, but SubsetJuliaVM also accepts Symbol
    ref1 = GlobalRef(Main, :add_numbers)
    result1 = ref1(3, 5)
    @test result1 == 8

    # Test 2: Call function with numeric argument
    ref2 = GlobalRef(Main, :square)
    result2 = ref2(7)
    @test result2 == 49
end

@testset "GlobalRef callable: shared dispatch resolver (Issue #3910)" begin
    ref_dispatch = GlobalRef(Main, :globalref_dispatch_3910)
    @test ref_dispatch(1) === :int
    @test ref_dispatch("value") === :any

    ref_abs_dispatch = GlobalRef(Base, :abs)
    @test ref_abs_dispatch(GlobalRefAbsBox3910(1)) === :box_abs

    ref_length_dispatch = GlobalRef(Base, :length)
    @test ref_length_dispatch(:x) == 3910
    @test ref_length_dispatch([1, 2, 3]) == 3
end

@testset "GlobalRef callable: calling Base builtins" begin
    # Test 3: Call Base.abs via GlobalRef
    ref_abs = GlobalRef(Base, :abs)
    @test ref_abs(-42) == 42

    # Test 4: Call Base.sqrt via GlobalRef
    ref_sqrt = GlobalRef(Base, :sqrt)
    @test ref_sqrt(16.0) == 4.0
    @test ref_sqrt(9) == 3.0
end

@testset "GlobalRef callable: printing functions" begin
    # Test 5: println via GlobalRef (returns nothing)
    ref_println = GlobalRef(Base, :println)
    result = ref_println("Testing GlobalRef println")
    @test result === nothing
end

@testset "GlobalRef callable: math functions" begin
    # Test 6: Trigonometric functions
    ref_sin = GlobalRef(Base, :sin)
    ref_cos = GlobalRef(Base, :cos)

    @test ref_sin(0.0) < 1e-10
    @test (ref_cos(0.0) - 1.0) < 1e-10

    # Test 7: exp and log
    ref_exp = GlobalRef(Base, :exp)
    ref_log = GlobalRef(Base, :log)

    @test (ref_exp(0.0) - 1.0) < 1e-10
    @test ref_log(1.0) < 1e-10

    # Test 8: floor and ceil
    ref_floor = GlobalRef(Base, :floor)
    ref_ceil = GlobalRef(Base, :ceil)

    @test ref_floor(3.7) == 3.0
    @test ref_ceil(3.2) == 4.0
end

true  # Test passed
