using Test

# Tests that negative array dimensions produce errors (Issue #2880)
# In Julia, negative dimensions throw ArgumentError

function zeros_negative_dim_caught()
    try
        zeros(-1)
    catch e
        return typeof(e) == ArgumentError
    end
    return false
end

function ones_negative_dim_caught()
    try
        ones(-1)
    catch e
        return typeof(e) == ArgumentError
    end
    return false
end

function zeros_f64_negative_dim_caught()
    try
        zeros(Float64, -1)
    catch e
        return typeof(e) == ArgumentError
    end
    return false
end

function ones_int64_negative_dim_caught()
    try
        ones(Int64, -1)
    catch e
        return typeof(e) == ArgumentError
    end
    return false
end

function zeros_multi_negative_dim_caught()
    try
        zeros(-1, -1)
    catch e
        return typeof(e) == ArgumentError
    end
    return false
end

@testset "negative array dimensions produce errors (Issue #2880)" begin
    @test zeros_negative_dim_caught()
    @test ones_negative_dim_caught()
    @test zeros_f64_negative_dim_caught()
    @test ones_int64_negative_dim_caught()
    @test zeros_multi_negative_dim_caught()
end

true
