# Split out of `signature_forward_reference_11025.jl` (Issue #10354 Phase 1a;
# see docs/vm/EXCEPTION_PARITY.md) to keep that fixture's other assertions in the
# nightly parity sweep (Issue #10246) while this case was still broken.
#
# THE GAP IS NOW CLOSED (Issue #11146, #10813 Phase 2a). These assertions were
# `@test_broken`, and the file carried `skip_julia_test = true`, because sjulia's
# `eval`-time method-signature elaboration did not implement typed parameters at
# all: it raised `VmError::NotImplemented`, which — per the Issue #8664 mapping —
# had no Julia exception object, so `typeof(caught)` was a bare `String`, not
# even an `Exception` subtype, let alone the `UndefVarError` upstream raises.
#
# Issue #10354's own note said these would "fail loudly (unexpectedly passed) the
# moment #11146 closes the gap, prompting removal of both the `@test_broken`
# wrapper and this file's `skip_julia_test` flag". That is exactly what happened;
# this is that removal.
#
# Upstream Julia evaluates a method signature's type annotations EAGERLY when the
# `eval`'d definition executes, so a forward reference (a type named in the
# signature but not yet defined) raises `UndefVarError` at the eval site. sjulia
# now mirrors that: `vm/builtins_macro/eval.rs::probe_eval_signature_annotations`
# probes every parameter annotation and every `where` bound (minus the binders
# themselves, which the `where` binds) before defining the method — the runtime
# sibling of the compiled path's `Instr::LoadAny` probes (Issues #10396/#11025).
#
# Both interpreters now agree on the OUTCOME, so the deliberate divergence is
# gone: `skip_julia_test` is removed from this file's manifest entry, and the
# assertions are real `@test_throws` (which, since Issue #10354 landed, actually
# checks the exception TYPE). Verified against julia 1.12.6.

using Test

@testset "forward-referenced annotation raises UndefVarError (Issues #11025, #11146)" begin
    @test_throws UndefVarError eval(:(f_forward_11025_eval(x::NotYetDefinedEvalGap11025) = 1))
    @test_throws UndefVarError eval(
        :(g_forward_11025_eval(x::T) where {T<:NotYetDefinedEvalGap11025} = 1),
    )
end

true
