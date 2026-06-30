# Test: Ref field access and show/repr (Issue #5130)

using Test

@testset "Ref field access and display" begin
    r = Ref(42)

    # The single field is `x`
    @assert fieldnames(typeof(r)) == (:x,)
    @assert fieldcount(Base.RefValue{Int}) == 1
    @assert r.x == 42
    @assert getfield(r, :x) == 42

    # repr / string render as Base.RefValue{T}(value)
    @assert repr(r) == "Base.RefValue{Int64}(42)"
    @assert string(r) == "Base.RefValue{Int64}(42)"

    @test (true)
end

true  # Test passed
