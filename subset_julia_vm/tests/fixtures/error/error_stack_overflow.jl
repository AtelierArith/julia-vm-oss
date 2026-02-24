using Test

# Tests that unbounded recursion raises a catchable StackOverflowError (Issue #2918)

function infinite_recurse()
    return 1 + infinite_recurse()
end

function stack_overflow_caught()
    caught = false
    try
        infinite_recurse()
    catch e
        caught = true
    end
    return caught
end

function mutual_a()
    return mutual_b()
end

function mutual_b()
    return mutual_a()
end

function mutual_recursion_caught()
    caught = false
    try
        mutual_a()
    catch e
        caught = true
    end
    return caught
end

@testset "stack overflow raises catchable error (Issue #2918)" begin
    @test stack_overflow_caught()
    @test mutual_recursion_caught()
end

true
