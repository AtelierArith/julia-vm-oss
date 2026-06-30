using Test

struct DynIterState3910
end

function iterate(x::DynIterState3910)
    return (10, 1)
end

function iterate(x::DynIterState3910, state::Int64)
    return (20, "s")
end

function iterate(x::DynIterState3910, state::String)
    return (30, 2)
end

function step_any_3910(x::Any, st::Any)
    return iterate(x, st)
end

@testset "IterateDynamic uses full iterate(collection, state) signature (Issue #3910)" begin
    x = DynIterState3910()
    r1 = step_any_3910(x, 1)
    r2 = step_any_3910(x, "s")

    @test r1[1] == 20
    @test r1[2] == "s"
    @test r2[1] == 30
    @test r2[2] == 2
end

true
