# Issue #11111: the #11025 forward-reference probes compare source-order
# ordinals (`span.definition_order`) between a signature annotation's type and
# the definition using it, skipping the probe only when the type's own order is
# strictly earlier. At the time of #11111, `definition_order` was 0 — meaning
# "not stamped" per `subset_julia_vm_ir::span::Span`'s own doc comment
# ("Ordinary expression spans keep zero") — for two classes of definition the
# #11025 lowering never threaded an ordinal through:
#
#   1. A LOCAL/nested function definition: `f(x::T) = 1` written inside a
#      `@testset`/`if`/`for`/`while`/... body (not a bare top-level statement)
#      is lowered by `lower_function_defs_to_stmt`, which never calls
#      `stamp_function_definitions`.
#   2. An `abstract type` declaration had no `stamp_*` counterpart to
#      `stamp_struct_definition`. Issue #11654 added that chronology so root
#      nominal declarations can activate in source order; this fixture retains
#      the nested-function order-0 coverage and the earlier-type regression.
#
# The #11025 comparison read a 0 order as "defined at the very first ordinal"
# instead of "unknown", so a nested function referencing an ALREADY-DEFINED
# Base abstract type (`AbstractDict`), Base parametric type (`Rational`), or
# earlier user struct looked like a forward reference and probe-failed with a
# spurious `UndefVarError` — even though nothing was actually forward. This
# broke main via PR #11082 (fixture_tests dispatch::chunk_002,
# rational::chunk_001, types_tests::chunk_001, type_inference::chunk_002).
#
# Verified against julia 1.12.6.

using Test

struct PointNested11111
    x::Float64
    y::Float64
end

@testset "nested function definitions referencing earlier types (Issue #11111)" begin
    # Base abstract type referenced from a function defined INSIDE a testset
    # body (not a top-level statement).
    dict_kind(d::AbstractDict) = "dict"
    @test dict_kind(Dict{String,Int}()) == "dict"

    # Base parametric type referenced from a nested function.
    rational_kind(r::Rational) = :rational
    @test rational_kind(3 // 4) === :rational

    # User struct (defined earlier, at true top level) referenced from a
    # nested function.
    swap_xy(pt::PointNested11111) = PointNested11111(pt.y, pt.x)
    swapped = swap_xy(PointNested11111(1.0, 2.0))
    @test swapped.x == 2.0
    @test swapped.y == 1.0
end

true
