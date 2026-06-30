using Test

# Issue #4968: representative public string-transformation helper signatures must
# report the same `Base.infer_effects` / `Base.infer_exception_type` records that
# upstream Julia infers, instead of sjulia's accidental proven-total fallback.
#
# Upstream infers most of these helpers to `EFFECTS_UNKNOWN`
# (`(!c,!e,!n,!t,!s,!m,!u,+o,!r)`, exception `Any`) because they carry no
# `@assume_effects` annotation and their bodies cannot be refined. A handful
# (`lstrip`, `repeat(::String,::Int)`) expose a more precise record. All values
# captured field-for-field from Julia 1.12.6 and verified with
# `bash scripts/fixture_julia_parity.sh`.

const _UNKNOWN_4968 = "(!c,!e,!n,!t,!s,!m,!u,+o,!r)"

@testset "infer_effects string transform helpers EFFECTS_UNKNOWN (#4968)" begin
    for f in (uppercase, lowercase, titlecase, strip, rstrip, chomp, chop, split)
        @test string(Base.infer_effects(f, Tuple{String})) == _UNKNOWN_4968
        @test Base.infer_exception_type(f, Tuple{String}) === Any
    end
    @test string(Base.infer_effects(join, Tuple{Vector{String}})) == _UNKNOWN_4968
    @test Base.infer_exception_type(join, Tuple{Vector{String}}) === Any
end

@testset "infer_effects lstrip / repeat string precise records (#4968)" begin
    # lstrip is effect-free + noub but otherwise imprecise.
    @test string(Base.infer_effects(lstrip, Tuple{String})) == "(!c,+e,!n,!t,!s,!m,+u,+o,+r)"
    @test Base.infer_exception_type(lstrip, Tuple{String}) === Any

    # repeat(::String, ::Int) is consistent + effect-free and terminates, but may
    # throw and is not task-state / inaccessible-mem proven.
    @test string(Base.infer_effects(repeat, Tuple{String,Int64})) == "(+c,+e,!n,+t,!s,!m,+u,+o,+r)"
    @test Base.infer_exception_type(repeat, Tuple{String,Int64}) === Any
end

@testset "infer_effects string helpers do not intercept non-string overloads (#4968)" begin
    # repeat(::Vector, ::Int) is a different method with different effects; it must
    # keep falling through to the proven-total default (not the string record).
    @test string(Base.infer_effects(repeat, Tuple{Vector{Int64},Int64})) !=
        "(+c,+e,!n,+t,!s,!m,+u,+o,+r)"
end

true
