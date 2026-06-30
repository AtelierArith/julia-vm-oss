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
