# Type assertion on arbitrary call expressions: `expr::T` (typeassert) (Issue #5193)
#
# `a::T` outside a declaration lowers to `typeassert(a, T)`: it returns `a`
# unchanged when `a isa T`, otherwise throws a TypeError. This must work for
# both literal types (`::Int`) and computed types (`::typeof(x)`), and on
# arbitrary call expressions (not just simple variables).

using Test

@testset "typeassert on call expressions (Issue #5193)" begin
    # literal type assertion on a call expression
    @test convert(Int, 2.0)::Int === 2

    # computed type assertion (typeof(x)) on a call expression
    x = 1
    @test convert(Int, 2.0)::typeof(x) === 2

    # assertion that returns the value unchanged
    @test (1 + 1)::Int === 2
    @test identity(3.0)::Float64 === 3.0

    # failing assertion throws TypeError
    @test_throws TypeError convert(Int, 2.0)::String
    @test_throws TypeError identity(2)::Float64

    # oftype now uses the upstream definition with the trailing `::typeof(x)`
    @test oftype(1, 2.0) === 2
    @test oftype(1.0, 2) === 2.0
    @test typeof(oftype(1.0, 2)) === Float64
end

true
