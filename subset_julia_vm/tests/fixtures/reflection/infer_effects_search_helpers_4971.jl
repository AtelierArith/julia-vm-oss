using Test

# Issue #4971: representative string search / index helper signatures must report
# the upstream `Base.infer_effects` / `Base.infer_exception_type` records.
#
# `findfirst` / `findnext` / `count` (string ∩ string) / `replace` (over an
# `AbstractString`) infer to `EFFECTS_UNKNOWN` (`(!c,!e,!n,!t,!s,!m,!u,+o,!r)`).
# `thisind` / `nextind`(::String, ::Int) expose the more precise index record
# `(!c,+e,!n,+t,!s,!m,+u,+o,+r)`. All surface exception `Any`. Values captured
# field-for-field from Julia 1.12.6.

const _UNKNOWN_4971 = "(!c,!e,!n,!t,!s,!m,!u,+o,!r)"

@testset "infer_effects string search helpers EFFECTS_UNKNOWN (#4971)" begin
    @test string(Base.infer_effects(findfirst, Tuple{Char,String})) == _UNKNOWN_4971
    @test Base.infer_exception_type(findfirst, Tuple{Char,String}) === Any
    @test string(Base.infer_effects(findnext, Tuple{String,String,Int64})) == _UNKNOWN_4971
    @test Base.infer_exception_type(findnext, Tuple{String,String,Int64}) === Any
    @test string(Base.infer_effects(count, Tuple{String,String})) == _UNKNOWN_4971
    @test Base.infer_exception_type(count, Tuple{String,String}) === Any
    @test string(Base.infer_effects(replace, Tuple{String,Pair{String,String}})) == _UNKNOWN_4971
    @test Base.infer_exception_type(replace, Tuple{String,Pair{String,String}}) === Any
end

@testset "infer_effects string index helpers precise record (#4971)" begin
    for f in (thisind, nextind)
        @test string(Base.infer_effects(f, Tuple{String,Int64})) == "(!c,+e,!n,+t,!s,!m,+u,+o,+r)"
        @test Base.infer_exception_type(f, Tuple{String,Int64}) === Any
    end
end

@testset "infer_effects search helpers do not intercept non-string overloads (#4971)" begin
    # count(::Function, ::Vector) and findfirst(::Function, ::Vector) are distinct
    # methods with different effects; they must keep falling through.
    @test string(Base.infer_effects(count, Tuple{typeof(iseven),Vector{Int64}})) != _UNKNOWN_4971
    @test string(Base.infer_effects(findfirst, Tuple{typeof(iseven),Vector{Int64}})) != _UNKNOWN_4971
end

true
