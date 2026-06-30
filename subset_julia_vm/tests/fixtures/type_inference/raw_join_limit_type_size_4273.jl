using Test

function type_inference_same_wide_branch_4273(
    x::Union{Int64, Float64, String, Bool, Char, Nothing, Missing, Symbol, UInt8},
    flag::Bool,
)
    flag ? x : x
end

@testset "raw comparison-aware join preserves known wide union (Issue #4273)" begin
    @test Base.infer_return_type(
        type_inference_same_wide_branch_4273,
        Tuple{Union{Int64, Float64, String, Bool, Char, Nothing, Missing, Symbol, UInt8}, Bool},
    ) == Union{Int64, Float64, String, Bool, Char, Nothing, Missing, Symbol, UInt8}
    @test Base.return_types(
        type_inference_same_wide_branch_4273,
        Tuple{Union{Int64, Float64, String, Bool, Char, Nothing, Missing, Symbol, UInt8}, Bool},
    )[1] == Union{Int64, Float64, String, Bool, Char, Nothing, Missing, Symbol, UInt8}
end

true
