using Test

function make_pair_with_operator_6461()
    return :g => *
end

@testset "Pair with bare operator RHS (Issue #6461)" begin
    p = :f => +
    @test p.first == :f
    @test p.second(1, 2) == 3

    q = make_pair_with_operator_6461()
    @test q.first == :g
    @test q.second(2, 3) == 6
end

true
