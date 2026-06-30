using Test

# Issue #4969: representative parse / string-conversion helper signatures must
# report the upstream `Base.infer_effects` / `Base.infer_exception_type` records.
#
# `parse` / `tryparse` (over an `AbstractString` source), `bitstring`,
# `unescape_string`, and `repr` infer to `EFFECTS_UNKNOWN`
# (`(!c,!e,!n,!t,!s,!m,!u,+o,!r)`, exception `Any`). `string(::Char)` is the one
# precise outlier: total except `notaskstate` / `inaccessiblememonly`, no
# exception. Values captured field-for-field from Julia 1.12.6.

const _UNKNOWN_4969 = "(!c,!e,!n,!t,!s,!m,!u,+o,!r)"

@testset "infer_effects parse / conversion helpers EFFECTS_UNKNOWN (#4969)" begin
    @test string(Base.infer_effects(parse, Tuple{Type{Int64},String})) == _UNKNOWN_4969
    @test Base.infer_exception_type(parse, Tuple{Type{Int64},String}) === Any
    @test string(Base.infer_effects(tryparse, Tuple{Type{Int64},String})) == _UNKNOWN_4969
    @test Base.infer_exception_type(tryparse, Tuple{Type{Int64},String}) === Any
    @test string(Base.infer_effects(parse, Tuple{Type{Float64},String})) == _UNKNOWN_4969
    @test Base.infer_exception_type(parse, Tuple{Type{Float64},String}) === Any
    @test string(Base.infer_effects(tryparse, Tuple{Type{Float64},String})) == _UNKNOWN_4969
    @test Base.infer_exception_type(tryparse, Tuple{Type{Float64},String}) === Any

    @test string(Base.infer_effects(bitstring, Tuple{Int64})) == _UNKNOWN_4969
    @test Base.infer_exception_type(bitstring, Tuple{Int64}) === Any
    @test string(Base.infer_effects(unescape_string, Tuple{String})) == _UNKNOWN_4969
    @test Base.infer_exception_type(unescape_string, Tuple{String}) === Any
    @test string(Base.infer_effects(repr, Tuple{String})) == _UNKNOWN_4969
    @test Base.infer_exception_type(repr, Tuple{String}) === Any
end

@testset "infer_effects string(::Char) precise record (#4969)" begin
    @test string(Base.infer_effects(string, Tuple{Char})) == "(+c,+e,+n,+t,!s,!m,+u,+o,+r)"
    @test Base.infer_exception_type(string, Tuple{Char}) === Union{}
end

true
