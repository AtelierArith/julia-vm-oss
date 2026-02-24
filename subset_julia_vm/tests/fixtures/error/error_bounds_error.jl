using Test

# Tests that out-of-bounds access raises catchable errors (Issue #2918)

function array_oob_caught()
    caught = false
    try
        a = [1, 2, 3]
        x = a[10]
    catch e
        caught = true
    end
    return caught
end

function array_negative_idx_caught()
    caught = false
    try
        a = [1, 2, 3]
        x = a[-1]
    catch e
        caught = true
    end
    return caught
end

function array_zero_idx_caught()
    caught = false
    try
        a = [1, 2, 3]
        x = a[0]
    catch e
        caught = true
    end
    return caught
end

function tuple_oob_caught()
    caught = false
    try
        t = (10, 20, 30)
        x = t[10]
    catch e
        caught = true
    end
    return caught
end

function valid_array_access_no_catch()
    caught = false
    try
        a = [1, 2, 3]
        x = a[2]
    catch e
        caught = true
    end
    return caught
end

@testset "out-of-bounds access raises catchable error (Issue #2918)" begin
    @test array_oob_caught()
    @test array_negative_idx_caught()
    @test array_zero_idx_caught()
    @test tuple_oob_caught()
    @test !valid_array_access_no_catch()
end

true
