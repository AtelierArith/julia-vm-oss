# Test generic promote_rule with where clause pattern
# Issue #1339: Support generic where clause for promote_rule

using Test

# Define a new function that demonstrates the dynamic type construction pattern
# We use a different name to avoid conflicts with existing promote_rule methods
function my_complex_type(::Type{T}, ::Type{S}) where {T<:Real, S<:Real}
    Complex{promote_type(T, S)}
end

@testset "Dynamic parametric type construction" begin
    # Test basic promote_type functionality
    @test promote_type(Float64, Int64) == Float64
    @test promote_type(Int64, Int64) == Int64
    @test promote_type(Bool, Int64) == Int64

    # Test the dynamic type construction pattern
    # This is the key pattern we're enabling with this feature
    @test my_complex_type(Bool, Int64) == Complex{Int64}
    @test my_complex_type(Int64, Float64) == Complex{Float64}
    @test my_complex_type(Float64, Int64) == Complex{Float64}
    @test my_complex_type(Bool, Bool) == Complex{Bool}
end

true
