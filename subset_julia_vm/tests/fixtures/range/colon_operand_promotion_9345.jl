using Test

# Issue #9345: range construction (`:`) must promote its endpoint operands the
# way upstream `(:)(start, step, stop)` does (`promote(start, step, stop)` first,
# base/range.jl), so the range type and element type match Julia. Two regressions
# are covered here:
#
#   1. `1:Int8(5)` was `UnitRange{Int8}` — it must be `UnitRange{Int64}` because
#      `promote(Int64, Int8) == Int64` (the widest signed endpoint wins).
#   2. `eltype(1:0.5:3)` / `eltype(0:0.5f0:6.0)` returned `Any` — the value's
#      `typeof` is the 4-parameter display form
#      `StepRangeLen{T, TwicePrecision{T}, TwicePrecision{T}, Int64}`, which the
#      1-parameter `eltype(::Type{StepRangeLen{T}})` method did not match.
@testset "range/colon operand promotion (Issue #9345)" begin
    # Integer endpoint promotion: promote(start, stop) picks the widest of the
    # shared signedness. The synthetic unit step must NOT force the width.
    @test typeof(1:Int8(5)) == UnitRange{Int64}
    @test eltype(1:Int8(5)) == Int64
    @test typeof(Int8(1):Int8(5)) == UnitRange{Int8}
    @test typeof(Int8(1):Int16(5)) == UnitRange{Int16}
    @test typeof(UInt8(1):UInt8(3)) == UnitRange{UInt8}

    # A step range promotes all three operands (element type is the join).
    @test eltype(Int8(1):Int8(1):Int16(5)) == Int16

    # eltype of float ranges must be the promoted float element, not Any.
    @test eltype(1:0.5:3) == Float64
    @test eltype(0:0.5f0:6.0) == Float64
    @test eltype(0f0:0.5f0:6f0) == Float32
    @test eltype(0.0:0.5:6.0) == Float64

    # length parity for a promoted float range.
    @test length(0:0.5f0:6.0) == 13
    @test collect(0:0.5f0:6.0) == [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0,
                                   3.5, 4.0, 4.5, 5.0, 5.5, 6.0]
end

true
