using Test

struct Pos6548{T}
    v::T
    function Pos6548{T}(v) where T<:Real
        new(v)
    end
end

@testset "inner constructor where bounds are enforced (Issue #6548)" begin
    @test Pos6548{Int64}(1).v == 1
    ok = try
        Pos6548{String}("x")
        "constructed"
    catch e
        isa(e, MethodError) ? "rejected" : "wrong-error"
    end
    @test ok == "rejected"
    ok == "rejected" || error("Pos6548{String} should be rejected by where T<:Real")
end

true
