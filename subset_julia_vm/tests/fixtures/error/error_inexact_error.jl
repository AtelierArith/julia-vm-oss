using Test

# Tests that inexact (lossy) type conversions raise catchable errors (Issue #2918)
# In Julia, converting a non-integer Float64 to Int64 throws InexactError

function inexact_float_to_int_caught()
    caught = false
    try
        x = Int64(3.14)
    catch e
        caught = true
    end
    return caught
end

function inexact_float_to_int32_caught()
    caught = false
    try
        x = Int32(2.7)
    catch e
        caught = true
    end
    return caught
end

function exact_float_no_catch()
    # Converting a whole-number float to Int64 is exact — no exception
    caught = false
    try
        x = Int64(3.0)
    catch e
        caught = true
    end
    return caught
end

@testset "inexact conversion raises catchable error (Issue #2918)" begin
    @test inexact_float_to_int_caught()
    @test inexact_float_to_int32_caught()
    @test !exact_float_no_catch()
end

true
