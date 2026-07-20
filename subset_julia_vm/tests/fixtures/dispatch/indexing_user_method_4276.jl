# Kept standalone: overrides Base methods on Base argument types, so the method
# table interaction is process-global and aggregation is order-dependent even
# under upstream julia (#5966 class; excluded from Issue #10238 module-wrap
# aggregation).
using Test

import Base: getindex, setindex!

getindex(v::Vector{Int64}, i::Int64) = 99
setindex!(v::Vector{Int64}, value::Int64, i::Int64) = :user_setindex_4276

@testset "array indexing uses user methods before fallback (Issue #4276)" begin
    @test getindex([1, 2, 3], 1) == 99
    @test ([1, 2, 3])[1] == 99
    @test getindex([1.0, 2.0], 1) == 1.0
    @test ([1.0, 2.0])[1] == 1.0

    xs = [1, 2, 3]
    @test setindex!(xs, 10, 1) === :user_setindex_4276
    @test xs == [1, 2, 3]
    xs[1] = 10
    @test xs == [1, 2, 3]

    ys = [1.0, 2.0]
    @test setindex!(ys, 3.0, 2) == [1.0, 3.0]
    @test ys == [1.0, 3.0]
end

true
