using Test

abstract type Shape5648 end
struct Box5648{T} end
f5648(x::T) where {T<:Shape5648} = "shape-bound"

@testset "catchable MethodError + exception types (Issue #5648)" begin
    items = Any[Box5648{Int}()]
    caught_type = ""
    for it in items
        try
            f5648(it)
        catch e
            caught_type = string(typeof(e))
        end
    end
    @test caught_type == "MethodError"

    de = try; sqrt(-1.0); catch e; e; end
    @test de isa DomainError
    @test typeof(de) == DomainError

    be = try; [1, 2, 3][10]; catch e; e; end
    @test be isa BoundsError

    dz = try; div(1, 0); catch e; e; end
    @test dz isa DivideError

    ee = try; error("boom"); catch e; e; end
    @test ee isa ErrorException
    @test ee.msg == "boom"
end
true
