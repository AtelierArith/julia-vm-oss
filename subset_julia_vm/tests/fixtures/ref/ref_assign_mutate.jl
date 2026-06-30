# Test: Ref mutation via [] and setindex! (Issue #5130)

using Test

@testset "Ref assignment and mutation" begin
    r = Ref(5)
    r[] = 10
    @assert r[] == 10

    # setindex!(r, v) is the zero-index assignment form
    setindex!(r, 42)
    @assert r[] == 42
    @assert getindex(r) == 42

    # In-place update operators
    r2 = Ref(0)
    r2[] += 1
    @assert r2[] == 1
    r2[] += 9
    @assert r2[] == 10

    # Reference (aliasing) semantics: aliases observe the write
    a = Ref(1)
    b = a
    b[] = 99
    @assert a[] == 99

    @test (true)
end

true  # Test passed
