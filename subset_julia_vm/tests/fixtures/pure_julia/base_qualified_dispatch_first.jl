# Module-qualified Base calls should still prefer Julia method dispatch before
# Rust builtin fallback routes (Issue #3861).

using Test

struct BaseQualifiedBox
    n::Int64
end

function Base.length(b::BaseQualifiedBox)
    return b.n + 100
end

function Base.keys(b::BaseQualifiedBox)
    return b.n + 200
end

function Base.values(b::BaseQualifiedBox)
    return b.n + 300
end

function Base.pairs(b::BaseQualifiedBox)
    return b.n + 400
end

@testset "Base-qualified dispatch-first public names (Issue #3861)" begin
    b = BaseQualifiedBox(5)
    @test Base.length(b) == 105
    @test Base.keys(b) == 205
    @test Base.values(b) == 305
    @test Base.pairs(b) == 405
end

true
