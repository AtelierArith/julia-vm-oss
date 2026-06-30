# Issue #4795: 'a':'e' (Char range) used to crash because the VM
# range machinery (`pop_f64_or_i64` in `Instr::MakeRangeLazy`) only
# accepted numeric operands.
#
# Fix slice: added `RangeElementType::Char` variant,
# `pop_f64_or_i64_or_char` helper that converts `Value::Char` to its
# codepoint as `f64`, and wired through `typed_element` /
# `Range[i]` / `RangeValue::collect` to convert codepoints back to
# `Char` on element materialization.
#
# Scope: this fixture covers the construction / length / indexing /
# collect / first / last surface. The for-loop iteration path
# (`for c in 'a':'e'`) is a separate code path that still produces
# Int64 loop variables — tracked as a follow-up under #4795 itself.

using Test

@testset "Char range basic construction does not crash (Issue #4795)" begin
    r = 'a':'e'
    @test length(r) == 5
end

@testset "Char range indexing returns Char (Issue #4795)" begin
    r = 'a':'e'
    @test r[1] === 'a'
    @test r[2] === 'b'
    @test r[5] === 'e'
end

@testset "Char range collect returns Vector{Char} (Issue #4795)" begin
    @test collect('a':'e') == ['a', 'b', 'c', 'd', 'e']
end

@testset "Char range first/last (Issue #4795)" begin
    r = 'a':'e'
    @test first(r) === 'a'
    @test last(r) === 'e'
end

@testset "Numeric range regression — Int/Float unchanged (Issue #4795)" begin
    # Make sure the Char arm doesn't break numeric ranges
    @test collect(1:3) == [1, 2, 3]
    @test eltype(collect(1:3)) === Int64
    @test first(0.0:1.0:5.0) === 0.0
end

true
