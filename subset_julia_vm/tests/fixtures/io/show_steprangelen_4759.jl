# Issue #4759: show/repr for StepRangeLen with float step (and LinRange)
# crashed because the Pure-Julia first/last/step(::StepRangeLen) methods
# access struct fields, but VM-native Value::Range is reported as
# StepRangeLen yet is not backed by struct fields. The fix forwards through
# an untyped helper that forces dynamic dispatch, which routes through the
# VM Range builtins for VM-native ranges.
#
# Cases covered here: non-whole-number floats only, to avoid #4760
# (whole-number Float64 ranges narrow first/step/last to Int64). LinRange
# coverage uses show/repr only, to avoid #4761 (print(buf, ::StructRef)
# and string(::StructRef) bypass the Pure-Julia show methods).

using Test

@testset "repr(StepRangeLen) (Issue #4759)" begin
    # VM-native Value::Range: previously crashed with
    # "GetField(0): expected struct, got Range in show(IO, CartesianIndex)"
    @test repr(1:0.5:3) == "1.0:0.5:3.0"
    @test repr(0:0.25:1) == "0.0:0.25:1.0"
    @test repr(0.0:0.5:5.0) == "0.0:0.5:5.0"

    # show into IOBuffer matches
    buf = IOBuffer()
    show(buf, 1:0.5:3)
    @test String(take!(buf)) == "1.0:0.5:3.0"

    # print and string round-trip the same form for VM-native ranges
    @test string(1:0.5:3) == "1.0:0.5:3.0"
end

@testset "repr(LinRange) (Issue #4759)" begin
    @test repr(LinRange(0.0, 1.0, 5)) == "LinRange{Float64}(0.0, 1.0, 5)"

    buf = IOBuffer()
    show(buf, LinRange(0.0, 1.0, 5))
    @test String(take!(buf)) == "LinRange{Float64}(0.0, 1.0, 5)"
end

@testset "no crash inside string interpolation (Issue #4759)" begin
    r = 1:0.5:3
    s = "range = $r"
    @test s == "range = 1.0:0.5:3.0"
end

true
