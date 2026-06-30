using MacroTools
using Test

@testset "MacroTools combinedef uses bare Dict dispatch (Issue #7632)" begin
    f(d::Dict) = 1
    d = MacroTools.splitdef(:(foo(x) = x + 2))

    @test typeof(d) == Dict{Symbol,Any}
    @test f(d) == 1
    @test MacroTools.combinedef(d).head == :function
end

true
