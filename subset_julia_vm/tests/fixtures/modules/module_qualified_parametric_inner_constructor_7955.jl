using Test

module QualifiedParametricInner7955

export Wrapped

struct Wrapped{T}
    x::T
    Wrapped(x::T) where T = new{T}(x + one(x))
end

end

@testset "qualified parametric inner constructor dispatch (PR #7955)" begin
    w = QualifiedParametricInner7955.Wrapped(41)
    @test typeof(w) == QualifiedParametricInner7955.Wrapped{Int64}
    @test getfield(w, 1) == 42
    # Named field access now resolves the field table for this module-qualified
    # parametric inner-constructor instance too (Issue #7958, previously errored).
    @test w.x == 42
end

true
