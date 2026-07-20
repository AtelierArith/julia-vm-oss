# Issue #9106: typeof(closure returned from a function) must be a
# per-definition-site singleton type, not the shared `Function` DataType,
# and `::typeof(f)` annotations must dispatch on closure-valued bindings.
#
# upstream Julia reports var"#make_fn##0#make_fn##1"{Int64}; sjulia reports
# typeof(<qualified nested name>). Exact name-mangling parity is not
# required — only the identity semantics below.

using Test

function make_fn(n)
    x -> x + n
end

function make_gn(n)
    x -> x * n
end

f = make_fn(5)
g = make_fn(10)
h = make_gn(2)

@testset "closure values still call correctly" begin
    @test f(3) == 8
    @test g(3) == 13
    @test h(3) == 6
end

@testset "typeof(closure) is a distinct singleton type" begin
    # Not the shared Function DataType any more.
    @test typeof(f) != Function
    # Two instances of the SAME closure template share one type.
    @test typeof(f) === typeof(g)
    # Closures from DIFFERENT definition sites have different types.
    @test typeof(f) !== typeof(h)
    @test string(typeof(f)) != string(typeof(h))
end

@testset "closures remain Functions" begin
    @test f isa Function
    @test map(f, [1, 2]) == [6, 7]
end

function call_fn(fn::typeof(f), x)
    fn(x)
end

@testset "::typeof(f) dispatch on closure-valued binding" begin
    @test call_fn(f, 3) == 8
    # g shares f's closure type, so it dispatches through the same method.
    @test call_fn(g, 3) == 13
    # h has a different closure type: no matching method.
    @test_throws MethodError call_fn(h, 3)
end

true
