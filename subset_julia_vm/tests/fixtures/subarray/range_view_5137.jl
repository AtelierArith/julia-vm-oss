using Test

@testset "Range view returns range slice (Issue #5137)" begin
    u = view(1:10, 2:4)
    @test typeof(u) == UnitRange{Int64}
    @test collect(u) == [2, 3, 4]
    @test length(u) == 3
    @test u[2] == 3

    s = view(1:2:15, 2:4)
    @test typeof(s) == StepRange{Int64,Int64}
    @test collect(s) == [3, 5, 7]
    @test length(s) == 3
    @test s[2] == 5

    o = view(Base.OneTo(10), 2:4)
    @test typeof(o) == UnitRange{Int64}
    @test collect(o) == [2, 3, 4]
    @test length(o) == 3
    @test o[2] == 3
end

true
