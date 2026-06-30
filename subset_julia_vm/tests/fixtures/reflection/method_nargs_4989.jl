# Method.nargs / CodeInfo.nargs include the function argument (Issue #4989)
using Test

f_nargs_4989(x, y) = x + y
g_nargs_4989(x) = x

@testset "Method/CodeInfo nargs includes function argument" begin
    m = first(methods(f_nargs_4989))
    # Upstream Julia: nargs includes the function object and is Int32.
    @test m.nargs == 3
    @test m.nargs isa Int32

    g = first(methods(g_nargs_4989))
    @test g.nargs == 2
    @test g.nargs isa Int32

    # CodeInfo.nargs derives the same function-inclusive value.
    ci = Base.code_lowered(f_nargs_4989, Tuple{Int64,Int64})[1]
    @test ci.nargs == 3

    cig = Base.code_lowered(g_nargs_4989, Tuple{Int64})[1]
    @test cig.nargs == 2

    cit = Base.code_typed(f_nargs_4989, Tuple{Int64,Int64})[1][1]
    @test cit.nargs == 3
end

true
