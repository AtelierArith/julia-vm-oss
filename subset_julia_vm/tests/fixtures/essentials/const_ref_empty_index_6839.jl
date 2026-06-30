# Regression test for Issue #6839.
#
# `name[]` (empty brackets) on a `const` global is `getindex(name)`, NOT the
# typed-empty-array literal `T[]`. The compiler used to treat `LOG[]` as
# `Vector{LOG}()` whenever `LOG` was an unrecognized identifier, so a `const`
# global `Ref` (and any variable bound to a type) read back an empty
# `Vector{Any}` instead of the stored value. The original issue surfaced it
# through an unrelated user `setindex!` array override, but the override is a
# red herring — the breakage is purely `const-global[]` reads.

using Test

const LOG = Ref(0)

@testset "const Ref empty-index read (Issue #6839)" begin
    # Initial value read through `[]`
    @test LOG[] == 0

    # Write then read back
    LOG[] = 99
    @test LOG[] == 99

    # Through a function body (LOG resolved as a global value binding there too)
    f(v) = (LOG[] = v; LOG[])
    @test f(7) == 7
    @test LOG[] == 7

    # The original issue MWE: a user `setindex!` array override must not change
    # any of the above (it is unrelated to Ref indexing).
    import Base: setindex!
    setindex!(xs::Vector{Int64}, v::Int, i::Int) = xs
    LOG[] = 42
    @test LOG[] == 42
end

@testset "variable bound to a type: T[] builds empty Vector{T} (Issue #6839)" begin
    T = Int
    a = T[]
    @test a isa Vector{Int}
    @test length(a) == 0

    # Literal type-name empty-array literals must keep working (regression guard).
    b = Int[]
    @test b isa Vector{Int}
    @test length(b) == 0

    c = Float64[]
    @test c isa Vector{Float64}
    @test length(c) == 0
end

true
