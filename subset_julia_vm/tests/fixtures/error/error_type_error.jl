using Test

# Tests that non-boolean values in boolean context raise catchable TypeError (Issue #2918)
# Julia is strict: only Bool is allowed in if/while conditions

function non_bool_in_if_caught()
    caught = false
    try
        x = 42
        if x  # TypeError: non-boolean (Int64) used in boolean context
            caught = false
        end
    catch e
        caught = true
    end
    return caught
end

function non_bool_in_while_caught()
    caught = false
    try
        while 42  # TypeError: non-boolean (Int64) used in boolean context
            break
        end
    catch e
        caught = true
    end
    return caught
end

function bool_condition_no_catch()
    # Boolean conditions work fine — catch block should NOT run
    caught = false
    try
        if true
            caught = false
        end
    catch e
        caught = true
    end
    return caught
end

@testset "non-boolean in boolean context raises catchable TypeError (Issue #2918)" begin
    @test non_bool_in_if_caught()
    @test non_bool_in_while_caught()
    @test !bool_condition_no_catch()
end

true
