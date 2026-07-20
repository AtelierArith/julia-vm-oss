# A Nothing-initialized local keeps a backing value when one branch widens it.

using Test

function destructure_branch_10819(take_branch, pair)
    result = nothing
    if take_branch
        ignored, result = pair
    end
    result
end

function scalar_branch_10819(take_branch, value)
    result = nothing
    if take_branch
        result = value
    end
    result
end

@testset "Nothing local survives conditional widening (Issue #10819)" begin
    @test destructure_branch_10819(false, (1, "assigned")) === nothing
    @test destructure_branch_10819(true, (1, "assigned")) == "assigned"
    @test scalar_branch_10819(false, 42) === nothing
    @test scalar_branch_10819(true, 42) == 42
end

true
