# Float16/Float32 colon ranges expose upstream StepRangeLen parameters.

using Test

function range_dispatch_key_10019(r::StepRangeLen{T,R,S,L}) where {T,R,S,L}
    return (T, R, S, L)
end

function range_surface_10019(r)
    return (
        string(typeof(r)),
        eltype(r),
        typeof(step(r)),
        step(r),
        typeof(first(r)),
        typeof(last(r)),
        typeof(r[2]),
        typeof(collect(r)),
        eltype(collect(r)),
        range_dispatch_key_10019(r),
    )
end

@testset "Float16 and Float32 colon StepRangeLen type parameters (#10019)" begin
    f16_unit = Float16(0):Float16(1)
    @test range_surface_10019(f16_unit) == (
        "StepRangeLen{Float16, Float64, Float64, Int64}",
        Float16,
        Float16,
        Float16(1),
        Float16,
        Float16,
        Float16,
        Vector{Float16},
        Float16,
        (Float16, Float64, Float64, Int64),
    )

    f16_step = Float16(0):Float16(0.5):Float16(1)
    @test range_surface_10019(f16_step) == (
        "StepRangeLen{Float16, Float64, Float64, Int64}",
        Float16,
        Float16,
        Float16(0.5),
        Float16,
        Float16,
        Float16,
        Vector{Float16},
        Float16,
        (Float16, Float64, Float64, Int64),
    )

    f32_step = 0.0f0:0.5f0:1.0f0
    @test range_surface_10019(f32_step) == (
        "StepRangeLen{Float32, Float64, Float64, Int64}",
        Float32,
        Float32,
        0.5f0,
        Float32,
        Float32,
        Float32,
        Vector{Float32},
        Float32,
        (Float32, Float64, Float64, Int64),
    )

    mixed_f16_f32 = Float16(0):0.5f0:1
    @test range_surface_10019(mixed_f16_f32) == (
        "StepRangeLen{Float32, Float64, Float64, Int64}",
        Float32,
        Float32,
        0.5f0,
        Float32,
        Float32,
        Float32,
        Vector{Float32},
        Float32,
        (Float32, Float64, Float64, Int64),
    )
end

true
