# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: reflection/infer_effects_basic_4274.jl =====
module Agg_infer_effects_basic_4274
using Test

# Issue #4274: minimal Base.infer_effects / Base.infer_exception_type surface.
# Simple pure user methods infer to the TOTAL effects representative with an
# empty (Union{}) exception type in upstream Julia 1.12.

reflection_effects_add_4274(x, y) = x + y
reflection_effects_sq_4274(x) = x * x
reflection_effects_ident_4274(x) = x

@testset "reflection infer_effects and infer_exception_type basic" begin
    # infer_exception_type for simple total functions is Union{}.
    @test Base.infer_exception_type(reflection_effects_add_4274, Tuple{Int64,Int64}) === Union{}
    @test Base.infer_exception_type(reflection_effects_sq_4274, Tuple{Int64}) === Union{}
    @test Base.infer_exception_type(reflection_effects_ident_4274, Tuple{Float64}) === Union{}

    # infer_effects returns an Effects object whose accessor fields match upstream.
    ef = Base.infer_effects(reflection_effects_add_4274, Tuple{Int64,Int64})

    # UInt8 bitfields default to ALWAYS_TRUE (0x00) for proven-total methods.
    @test ef.consistent === 0x00
    @test ef.effect_free === 0x00
    @test ef.inaccessiblememonly === 0x00
    @test ef.noub === 0x00
    @test ef.nonoverlayed === 0x00

    # Bool fields are true for proven-total methods.
    @test ef.nothrow === true
    @test ef.terminates === true
    @test ef.notaskstate === true
    @test ef.nortcall === true

    # Custom show matches the upstream Effects key format exactly.
    @test string(ef) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"

    # Field names match upstream Compiler.Effects in order.
    @test fieldnames(typeof(ef)) === (:consistent, :effect_free, :nothrow, :terminates,
        :notaskstate, :inaccessiblememonly, :noub, :nonoverlayed, :nortcall)

    # Single-argument forms reflect over all methods.
    @test Base.infer_exception_type(reflection_effects_ident_4274) === Union{}
    @test string(Base.infer_effects(reflection_effects_ident_4274)) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
end
end # module Agg_infer_effects_basic_4274

# ===== source: reflection/infer_effects_bitwise_4274.jl =====
module Agg_infer_effects_bitwise_4274
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
end # module Agg_infer_effects_bitwise_4274

# ===== source: reflection/infer_effects_direct_kw_value_8441.jl =====
module Agg_infer_effects_direct_kw_value_8441
using Test

const TOTAL_DIRECT_KW_8441 = "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"

direct_kw_leaf_8441(x) = x
direct_kw_accept_8441(x; y=x) = y
direct_kw_wrapper_8441(x) = direct_kw_accept_8441(x; y=direct_kw_leaf_8441(x))

@testset "infer_effects derives direct keyword value callees (Issue #8441)" begin
    @test string(Base.infer_effects(direct_kw_wrapper_8441, Tuple{Int64})) == TOTAL_DIRECT_KW_8441
end
end # module Agg_infer_effects_direct_kw_value_8441

# ===== source: reflection/infer_effects_foldable_minmax_8441.jl =====
module Agg_infer_effects_foldable_minmax_8441
using Test

const TOTAL_8441 = "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"

foldable_min_8441(x, y) = min(x, y)
foldable_max_8441(x, y) = max(x, y)

@testset "infer_effects derives foldable min/max wrappers (Issue #8441)" begin
    @test string(Base.infer_effects(foldable_min_8441, Tuple{Int64,Int64})) == TOTAL_8441
    @test string(Base.infer_effects(foldable_max_8441, Tuple{Int64,Int64})) == TOTAL_8441
end
end # module Agg_infer_effects_foldable_minmax_8441

# ===== source: reflection/infer_effects_module_qualified_minmax_8441.jl =====
module Agg_infer_effects_module_qualified_minmax_8441
using Test

const TOTAL_QUALIFIED_8441 = "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"

qualified_min_8441(x, y) = Base.min(x, y)
qualified_max_8441(x, y) = Base.max(x, y)

@testset "infer_effects derives module-qualified min/max wrappers (Issue #8441)" begin
    @test string(Base.infer_effects(qualified_min_8441, Tuple{Int64,Int64})) == TOTAL_QUALIFIED_8441
    @test string(Base.infer_effects(qualified_max_8441, Tuple{Int64,Int64})) == TOTAL_QUALIFIED_8441
end
end # module Agg_infer_effects_module_qualified_minmax_8441

# ===== source: reflection/infer_effects_parse_helpers_4969.jl =====
module Agg_infer_effects_parse_helpers_4969
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
end # module Agg_infer_effects_parse_helpers_4969

# ===== source: reflection/infer_effects_search_helpers_4971.jl =====
module Agg_infer_effects_search_helpers_4971
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
end # module Agg_infer_effects_search_helpers_4971

# ===== source: reflection/infer_effects_string_helpers_4968.jl =====
module Agg_infer_effects_string_helpers_4968
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
end # module Agg_infer_effects_string_helpers_4968

true
