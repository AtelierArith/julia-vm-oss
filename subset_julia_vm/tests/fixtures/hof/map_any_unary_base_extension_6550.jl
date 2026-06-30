using Test

import Base: abs

struct Holder6550{T}
    v::T
end

abs(h::Holder6550) = "holder-any"

@testset "map unary Base extension over Vector{Any} stays unary (Issue #6550)" begin
    hs = Any[Holder6550(3), Holder6550("s")]
    mapped = map(abs, hs)
    @test mapped == ["holder-any", "holder-any"]
    mapped == ["holder-any", "holder-any"] ||
        error("map(abs, ::Vector{Any}) should call the unary user extension")
end

true
