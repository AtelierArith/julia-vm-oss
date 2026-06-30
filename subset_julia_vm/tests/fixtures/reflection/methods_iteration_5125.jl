# Issue #5125: methods(f) reflection — method count + iteration/collect.
#
# Verifies that methods(f) for user-defined generic functions reports the
# correct method count and supports iteration (for ... in), collect, isempty,
# and indexing into the collected result. This mirrors upstream Julia 1.12
# behavior for the portable surface (count + iteration); the exact MethodList
# type name and the precise `Method` show formatting (file/line) are deferred
# (see UNIMPLEMENTED.md / Issue #5125).

using Test

# A function with three single-argument methods of different specificity.
function quux5125(x)
    x
end
function quux5125(x::Int64)
    x + 1
end
function quux5125(x::Float64)
    x * 2.0
end

# A function with a single method.
solo5125(x) = x

# A function mixing fixed arity and varargs.
poly5125(x::Int) = 1
poly5125(x::Int, y::Int) = 2
poly5125(x...) = 3

# A where-parametrized single-method function.
id5125(x::T) where T = x

@testset "methods(f): count" begin
    @test length(methods(quux5125)) == 3
    @test length(methods(solo5125)) == 1
    @test length(methods(poly5125)) == 3
    @test length(methods(id5125)) == 1
end

@testset "methods(f): isempty" begin
    @test isempty(methods(quux5125)) == false
    @test isempty(methods(solo5125)) == false
end

@testset "methods(f): iteration with for-loop" begin
    counted = 0
    for m in methods(quux5125)
        counted += 1
    end
    @test counted == 3

    # Every iterated entry belongs to the same generic function.
    all_same = true
    for m in methods(quux5125)
        if m.name !== :quux5125
            all_same = false
        end
    end
    @test all_same == true
end

@testset "methods(f): collect and index" begin
    ms = collect(methods(quux5125))
    @test length(ms) == 3
    @test ms[1].name === :quux5125

    poly = collect(methods(poly5125))
    @test length(poly) == 3
end

@testset "methods(f): iteration over varargs/fixed mix" begin
    # Accumulate the function-inclusive nargs across all matched methods.
    # poly5125 methods have nargs 2 (x::Int), 3 (x::Int,y::Int), 2 (x...),
    # which sums to 7 and matches upstream Julia 1.12.
    total = 0
    for m in methods(poly5125)
        total += m.nargs
    end
    @test total == 7
end

true
