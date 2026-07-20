# range(start, stop; length) / range(start; step, length) TwicePrecision
# parity for Float32/Float16/narrow-int endpoints (Issue #9509), upstream
# julia/base/twiceprecision.jl `range_start_stop_length` /
# `range_start_step_length`. Verified against julia 1.12.

using Test

@testset "range(start, stop; length) Float32 endpoints (Issue #9509)" begin
    r = range(0f0, 1f0, length=3)
    @test string(typeof(r)) == "StepRangeLen{Float32, Float64, Float64, Int64}"
    @test collect(r) == Float32[0.0, 0.5, 1.0]
    @test eltype(r) === Float32
    @test first(r) === 0.0f0
    @test step(r) === 0.5f0
    @test last(r) === 1.0f0
    @test r[2] === 0.5f0
    @test repr(r) == "0.0f0:0.5f0:1.0f0"
    # Non-decade endpoints: values collapse through the Float64 ref/step pair.
    r2 = range(0.1f0, 0.7f0, length=7)
    @test collect(r2) == Float32[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7]
end

@testset "range(start, stop; length) Float16 endpoints (Issue #9509)" begin
    r = range(Float16(0), Float16(1), length=5)
    @test string(typeof(r)) == "StepRangeLen{Float16, Float64, Float64, Int64}"
    @test collect(r) == Float16[0.0, 0.25, 0.5, 0.75, 1.0]
    @test eltype(r) === Float16
    @test step(r) === Float16(0.25)
end

@testset "range(start, stop; length) narrow-int endpoints (Issue #9509)" begin
    r = range(Int32(0), Int32(1), length=3)
    @test string(typeof(r)) ==
          "StepRangeLen{Float64, Base.TwicePrecision{Float64}, Base.TwicePrecision{Float64}, Int64}"
    @test collect(r) == [0.0, 0.5, 1.0]
    r8 = range(UInt8(0), UInt8(10), length=3)
    @test string(typeof(r8)) ==
          "StepRangeLen{Float64, Base.TwicePrecision{Float64}, Base.TwicePrecision{Float64}, Int64}"
    @test collect(r8) == [0.0, 5.0, 10.0]
    # Mixed endpoints promote before dispatch (upstream range.jl:601).
    rm = range(Int32(1), 2.5f0, length=4)
    @test string(typeof(rm)) == "StepRangeLen{Float32, Float64, Float64, Int64}"
    @test collect(rm) == Float32[1.0, 1.5, 2.0, 2.5]
end

@testset "range(start; step, length) TwicePrecision form (Issue #9509)" begin
    r = range(1.0, step=0.5, length=5)
    @test string(typeof(r)) ==
          "StepRangeLen{Float64, Base.TwicePrecision{Float64}, Base.TwicePrecision{Float64}, Int64}"
    @test collect(r) == [1.0, 1.5, 2.0, 2.5, 3.0]
    @test step(r) === 0.5
    @test last(r) === 3.0
    @test repr(r) == "1.0:0.5:3.0"
    # The TwicePrecision path keeps decimal steps exact.
    rd = range(0.0, step=0.1, length=4)
    @test rd[3] === 0.2
    @test rd[4] === 0.3
    r32 = range(1f0, step=0.5f0, length=5)
    @test string(typeof(r32)) == "StepRangeLen{Float32, Float64, Float64, Int64}"
    @test collect(r32) == Float32[1.0, 1.5, 2.0, 2.5, 3.0]
    r16 = range(Float16(1), step=Float16(0.5), length=4)
    @test string(typeof(r16)) == "StepRangeLen{Float16, Float64, Float64, Int64}"
    @test collect(r16) == Float16[1.0, 1.5, 2.0, 2.5]
    # Mixed argument types promote (upstream twiceprecision.jl:439-446).
    rmix = range(1, step=2.5, length=3)
    @test string(typeof(rmix)) ==
          "StepRangeLen{Float64, Base.TwicePrecision{Float64}, Base.TwicePrecision{Float64}, Int64}"
    @test collect(rmix) == [1.0, 3.5, 6.0]
    rmix32 = range(1, step=0.5f0, length=3)
    @test string(typeof(rmix32)) == "StepRangeLen{Float32, Float64, Float64, Int64}"
    @test collect(rmix32) == Float32[1.0, 1.5, 2.0]
end

@testset "length-defined range edge cases (Issue #9509)" begin
    # Empty and singleton lengths.
    r0 = range(0f0, 1f0, length=0)
    @test length(r0) == 0
    @test repr(r0) == "0.0f0:-1.0f0:1.0f0"
    @test step(r0) === -1.0f0
    re = range(1.0, step=0.5, length=0)
    @test length(re) == 0
    @test repr(re) == "1.0:0.5:0.5"
    # Zero step is valid for the length-defined forms (no colon equivalent).
    rz = range(1.0, step=0.0, length=3)
    @test collect(rz) == [1.0, 1.0, 1.0]
    @test repr(rz) == "StepRangeLen(1.0, 0.0, 3)"
    # show(::StepRangeLen) zero-step constructor form (Issue #11440).
    @test repr(range(1.0, 1.0, length=3)) == "StepRangeLen(1.0, 0.0, 3)"
    @test repr(range(0f0, 0f0, length=2)) == "StepRangeLen(0.0f0, 0.0f0, 2)"
    # Upstream argument errors.
    @test_throws ArgumentError range(0f0, 1f0, length=1)
    @test_throws ArgumentError range(0.5f0, 1f0, length=-1)
    @test_throws ArgumentError range(1.0, step=0.5, length=-2)
    @test_throws ArgumentError range(Float16(0), Float16(1), length=-1)
end

@testset "IteratorSize of TwicePrecision StepRangeLen (Issue #11443)" begin
    @test typeof(Base.IteratorSize(0.1:0.1:0.3)) === Base.HasShape{1}
    @test typeof(Base.IteratorSize(range(1.0, step=0.5, length=3))) === Base.HasShape{1}
    @test typeof(Base.IteratorSize(range(0f0, 1f0, length=3))) === Base.HasShape{1}
end

@testset "non-IEEE element types keep LinRange (Issue #9509)" begin
    rq = range(1//2, 3//2, length=3)
    @test rq isa LinRange
    @test eltype(rq) === Rational{Int64}
    rb = range(big(1), big(2), length=3)
    @test rb isa LinRange
    @test eltype(rb) === BigFloat
end

true
