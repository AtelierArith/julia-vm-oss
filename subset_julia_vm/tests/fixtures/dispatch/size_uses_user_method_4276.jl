using Test

import Base: size

size(x::Vector{Int64}) = (99,)

size_any_4276(x::Any) = size(x)

@testset "size uses user method before fallback (Issue #4276)" begin
    @test size([1, 2, 3]) == (99,)
    @test size_any_4276([1, 2, 3]) == (99,)

    @test size([1.0, 2.0]) == (2,)
    @test size_any_4276([1.0, 2.0]) == (2,)
end

true
