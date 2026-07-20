# Kept standalone: overrides Base methods on Base argument types, so the method
# table interaction is process-global and aggregation is order-dependent even
# under upstream julia (#5966 class; excluded from Issue #10238 module-wrap
# aggregation).
using Test

import Base: map

map(::typeof(identity), xs::Vector{Int64}) = [:vector]
map(::typeof(identity), xs::Matrix{Int64}) = [:matrix]

map_matrix_any_4276(f, xs::Any) = map(f, xs)

@testset "map matrix rank dispatch before fallback (Issue #4276)" begin
    A = reshape([1, 2, 3, 4], 2, 2)

    @test map(identity, A) == [:matrix]
    @test map_matrix_any_4276(identity, A) == [:matrix]
    @test map(identity, [1, 2]) == [:vector]
end

true
