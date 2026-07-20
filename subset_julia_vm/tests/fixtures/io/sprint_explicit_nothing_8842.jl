using Test

function sprint_explicit_nothing_8842(io, x)
    print(io, x)
    nothing
end

function sprint_return_nothing_8842(io, x)
    print(io, x)
    return nothing
end

@testset "sprint returns IOBuffer contents when function returns nothing (Issue #8842)" begin
    @test sprint(sprint_explicit_nothing_8842, 1) == "1"
    @test sprint(sprint_return_nothing_8842, 1) == "1"
    @test sprint(sprint_explicit_nothing_8842, [:x => 10]) == "[:x => 10]"
    @test sprint(sprint_return_nothing_8842, [:x => 10]) == "[:x => 10]"
end

true
