using Test

top_result_7667 = begin
    top_inner_7667 = nothing
    false
end

last_assign_result_7667 = begin
    top_last_inner_7667 = 7
end

function local_begin_rhs_assignments_7667()
    local_result_7667 = begin
        local_inner_7667 = 40
        local_inner_7667 + 2
    end

    local_last_result_7667 = begin
        local_last_inner_7667 = 11
    end

    local_result_7667 == 42 &&
        local_inner_7667 == 40 &&
        local_last_result_7667 == 11 &&
        local_last_inner_7667 == 11
end

@testset "begin RHS assignment scope (Issue #7667)" begin
    @test top_result_7667 == false
    @test top_inner_7667 === nothing
    @test last_assign_result_7667 == 7
    @test top_last_inner_7667 == 7
    @test local_begin_rhs_assignments_7667()
end

true
