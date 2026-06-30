using Test

@testset "Any-typed GlobalRef exposes mod and name fields (Issue #7743)" begin
    g = GlobalRef(Core, Symbol("@doc"))
    x = Any[g][1]
    @test x == g
    @test x.mod == Core
    @test x.name == Symbol("@doc")
end

true
