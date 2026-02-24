using Test

# Tests that integer division by zero raises a catchable error (Issue #2918)

function div_by_zero_caught()
    caught = false
    try
        x = 1 ÷ 0
    catch e
        caught = true
    end
    return caught
end

function div_fn_by_zero_caught()
    caught = false
    try
        x = div(7, 0)
    catch e
        caught = true
    end
    return caught
end

function div_by_zero_no_catch()
    # When no exception, catch block should not run
    caught = false
    try
        x = 10 ÷ 2
    catch e
        caught = true
    end
    return caught
end

@testset "division by zero raises catchable error (Issue #2918)" begin
    @test div_by_zero_caught()
    @test div_fn_by_zero_caught()
    @test !div_by_zero_no_catch()
end

true
