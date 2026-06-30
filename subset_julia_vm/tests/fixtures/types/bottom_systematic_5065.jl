# Issue #5065: systematic treatment of Union{} (Bottom).
# Bottom is the empty type: a subtype of every type, the zero element of
# typeintersect, and the empty-Union normal form. `const Bottom = Union{}`
# (essentials.jl) names it. This fixture pins that behaviour against upstream
# Julia. A local `const Bottom = Union{}` keeps parity in both runtimes: it is
# defined in Base (not exported to Main upstream), so binding it locally is the
# portable way to reference the name identically under sjulia and julia.

using Test

const Bottom = Union{}

@testset "Bottom (Union{}) systematic semantics" begin
    # 1. Bottom is the canonical name for the empty Union.
    @test Bottom === Union{}

    # 2. Bottom <: T holds for every type T (Bottom is the lattice bottom).
    @test (Bottom <: Int) == true
    @test (Bottom <: Number) == true
    @test (Bottom <: String) == true
    @test (Bottom <: Any) == true
    @test (Bottom <: Union{Int, Float64}) == true
    @test (Bottom <: Bottom) == true

    # 3. T <: Bottom holds only when T === Bottom.
    @test (Int <: Bottom) == false
    @test (Any <: Bottom) == false
    @test (Number <: Bottom) == false

    # 4. typeintersect: Bottom is the zero element; disjoint types meet at Bottom.
    @test typeintersect(Int, String) === Bottom
    @test typeintersect(Int, Bottom) === Bottom
    @test typeintersect(Bottom, Number) === Bottom
    @test typeintersect(Bottom, Bottom) === Bottom
    # Non-disjoint intersection is unaffected.
    @test typeintersect(Int, Integer) === Int

    # 5. Empty-Union normalization: Bottom is absorbed / collapsed.
    @test Union{Int} === Int
    @test Union{Bottom, Int} === Int
    @test Union{Int, Bottom} === Int
    @test Union{Bottom} === Bottom
    @test Union{Bottom, Bottom} === Bottom

    # 6. isa: no value is an instance of Bottom.
    @test isa(1, Bottom) == false
    @test isa("x", Bottom) == false
end

true
