using Test

reflection_split_pick_4287(x::Int64) = 1
reflection_split_pick_4287(x::String) = "s"

function reflection_split_caller_4287(b::Bool)
    return reflection_split_pick_4287(b ? 1 : "x")
end

@testset "return_types preserves small union-split dispatch" begin
    rts = Base.return_types(reflection_split_caller_4287, (Bool,))
    @test length(rts) == 1
    @test rts[1] == Union{Int64, String}
    @test Base.infer_return_type(reflection_split_caller_4287, (Bool,)) == Union{Int64, String}
end

true
