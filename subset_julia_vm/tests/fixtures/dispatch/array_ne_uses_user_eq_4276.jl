# Kept standalone: overrides Base methods on Base argument types, so the method
# table interaction is process-global and aggregation is order-dependent even
# under upstream julia (#5966 class; excluded from Issue #10238 module-wrap
# aggregation).
using Test

Base.:(==)(a::Vector{Int64}, b::Vector{Int64}) = false

@testset "array != uses generic !(==) dispatch (Issue #4276)" begin
    @test ([1, 2] == [1, 2]) == false
    @test ([1, 2] != [1, 2]) == true
end

true
