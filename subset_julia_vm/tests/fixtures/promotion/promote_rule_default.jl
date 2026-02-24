# Test promote_rule returns Union{} (Bottom) for undefined type pairs
# The default fallback should return Union{}

using Test

@testset "promote_rule(String, Int64) === Union{} (undefined type pair)" begin

    # First verify the default fallback works
    r = promote_rule(String, Int64)
    @test (r === Union{})
end

true  # Test passed
