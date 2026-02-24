using Test

# Tests that negative array dimensions produce errors (Issue #2880)
# In Julia, negative dimensions throw ArgumentError

function zeros_negative_dim_caught()
    caught = false
    try
        zeros(-1)
    catch e
        caught = true
    end
    return caught
end

function ones_negative_dim_caught()
    caught = false
    try
        ones(-1)
    catch e
        caught = true
    end
    return caught
end

function zeros_f64_negative_dim_caught()
    caught = false
    try
        zeros(Float64, -1)
    catch e
        caught = true
    end
    return caught
end

@testset "negative array dimensions produce errors (Issue #2880)" begin
    @test zeros_negative_dim_caught()
    @test ones_negative_dim_caught()
    @test zeros_f64_negative_dim_caught()
end

true
