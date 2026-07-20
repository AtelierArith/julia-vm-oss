using Test

# Issue #9322: a range with mixed floating-point widths must take the
# promotion JOIN of start/step/stop (like Base.colon in base/range.jl), not the
# step's width. `0:0.5f0:6.0` (Int + Float32 + Float64) is a Float64 range;
# `0f0:0.5f0:6f0` (all Float32) must STAY Float32.
@testset "mixed float-width range promotion (Issue #9322)" begin
    expected = [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0]

    # Int + Float32 + Float64 → Float64 (the reported bug).
    r1 = 0:0.5f0:6.0
    @test eltype(collect(r1)) == Float64
    @test collect(r1) == expected
    @test all(x -> x isa Float64, collect(r1))
    @test length(r1) == 13
    @test last(r1) == 6.0

    # Float32 + Float64 + Int → Float64 (order-independent).
    r2 = 0f0:0.5:6
    @test eltype(collect(r2)) == Float64
    @test collect(r2) == expected
    @test length(r2) == 13
    @test last(r2) == 6.0

    # Pure Float32 must NOT be widened to Float64.
    r3 = 0f0:0.5f0:6f0
    @test eltype(collect(r3)) == Float32
    @test collect(r3) == Float32.(expected)
    @test all(x -> x isa Float32, collect(r3))

    # Int + Float32 (no Float64) stays Float32.
    r4 = 0:0.5f0:6
    @test eltype(collect(r4)) == Float32

    # Float16 combined with a wider float promotes to that wider float.
    r5 = Float16(0):0.5f0:6       # Float16 + Float32 → Float32
    @test eltype(collect(r5)) == Float32
    r6 = Float16(0):Float16(0.5):6.0   # Float16 + Float64 → Float64
    @test eltype(collect(r6)) == Float64

    # Pure Float64 stays Float64.
    r7 = 0.0:0.5:6.0
    @test eltype(collect(r7)) == Float64
end

true
