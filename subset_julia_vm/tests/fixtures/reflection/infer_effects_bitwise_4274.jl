using Test

# Issue #4274: representative bitwise / shift integer operations must report the
# upstream `Base.infer_effects` / `Base.infer_exception_type` records.
#
# These bit-manipulation operators wrap `Base.and_int` / `Base.or_int` /
# `Base.xor_int` / `Core.Intrinsics` shift intrinsics over `Integer` arguments:
# they access no externally accessible mutable memory, never throw, and are
# consistent + effect-free. Upstream Julia 1.12.6 infers `EFFECTS_TOTAL` =
# `(+c,+e,+n,+t,+s,+m,+u,+o,+r)` with exception type `Union{}` for every covered
# integer signature (Int64, UInt64, Bool, and mixed-width pairs all resolve
# identically). The classification is keyed by name AND integer argument types
# so non-integer overloads keep falling through unchanged.
#
# Only the operator function values (`&`, `|`, `~`, `<<`, `>>`, `>>>`, `xor`) are
# exercised: the named bit-count helpers (`count_ones`, `leading_zeros`,
# `bitrotate`, …) are not yet reflectable as first-class function values in the
# subset. Values captured field-for-field from Julia 1.12.6.

const _TOTAL = "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"

@testset "infer_effects binary bitwise ops total (#4274)" begin
    @test string(Base.infer_effects(xor, Tuple{Int64,Int64})) == _TOTAL
    @test Base.infer_exception_type(xor, Tuple{Int64,Int64}) === Union{}
    @test string(Base.infer_effects(xor, Tuple{UInt64,UInt64})) == _TOTAL
    @test string(Base.infer_effects(xor, Tuple{Bool,Bool})) == _TOTAL

    @test string(Base.infer_effects(&, Tuple{Int64,Int64})) == _TOTAL
    @test Base.infer_exception_type(&, Tuple{Int64,Int64}) === Union{}
    @test string(Base.infer_effects(&, Tuple{UInt64,UInt64})) == _TOTAL
    @test string(Base.infer_effects(&, Tuple{Bool,Bool})) == _TOTAL

    @test string(Base.infer_effects(|, Tuple{Int64,Int64})) == _TOTAL
    @test Base.infer_exception_type(|, Tuple{Int64,Int64}) === Union{}
    @test string(Base.infer_effects(|, Tuple{UInt64,UInt64})) == _TOTAL
    @test string(Base.infer_effects(|, Tuple{Bool,Bool})) == _TOTAL
end

@testset "infer_effects bitwise negation total (#4274)" begin
    @test string(Base.infer_effects(~, Tuple{Int64})) == _TOTAL
    @test Base.infer_exception_type(~, Tuple{Int64}) === Union{}
    @test string(Base.infer_effects(~, Tuple{UInt64})) == _TOTAL
    @test Base.infer_exception_type(~, Tuple{UInt64}) === Union{}
end

@testset "infer_effects shift operations total (#4274)" begin
    @test string(Base.infer_effects(<<, Tuple{Int64,Int64})) == _TOTAL
    @test Base.infer_exception_type(<<, Tuple{Int64,Int64}) === Union{}
    @test string(Base.infer_effects(<<, Tuple{Int64,UInt64})) == _TOTAL

    @test string(Base.infer_effects(>>, Tuple{Int64,Int64})) == _TOTAL
    @test Base.infer_exception_type(>>, Tuple{Int64,Int64}) === Union{}
    @test string(Base.infer_effects(>>, Tuple{Int64,UInt64})) == _TOTAL

    @test string(Base.infer_effects(>>>, Tuple{Int64,Int64})) == _TOTAL
    @test Base.infer_exception_type(>>>, Tuple{Int64,Int64}) === Union{}
    @test string(Base.infer_effects(>>>, Tuple{Int64,UInt64})) == _TOTAL
end

true
