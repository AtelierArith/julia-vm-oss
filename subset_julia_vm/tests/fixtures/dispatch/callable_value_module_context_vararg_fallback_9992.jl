# Prevention coverage for Issue #9992: combines the three callable-value
# dispatch axes from the #9979 regression class in ONE call site — a Function
# singleton value imported from a user MODULE (standing in for a package
# context), competing against a same-arity `Any` candidate and a broad
# vararg `Any...` catch-all. The three `#[test]`s guarding
# `resolve_callable_value_candidates` in
# `subset_julia_vm_types/src/inference_core/dispatch_resolver.rs`
# (`callable_value_candidates_prefer_function_singleton_over_any_issue_9979`,
# `..._prefer_array_family_over_any_vararg_issue_9979`,
# `..._prefer_fixed_arity_over_vararg_tie_issue_9979`) each exercise the
# shared resolver directly with synthetic candidates; this fixture exercises
# the SAME invariant end-to-end through the real VM/module-dispatch path, so
# a regression that fixes future HOF ordering symptoms by widening VM-side
# runtime candidate SORTING (instead of routing through the shared resolver)
# — the exact failure mode #9979's root cause warns against for
# package/metaprogramming call sites — would surface here even if the
# synthetic unit tests still pass.
#
# NOTE: the function value is captured via its bare, `using`-imported name
# (`h = transform9992`), NOT explicit module qualification (`h =
# Pkg9992Combine.transform9992`). The qualified-access form was found to hit
# an unrelated bug (Issue #10077: a module-qualified function value failed
# `isa Function` in sjulia, while the same function accessed via its bare
# imported name did not) while writing this fixture. That bug is now fixed
# (see `dispatch/qualified_function_value_identity_10077.jl`) — this fixture
# still intentionally uses the bare access path so it keeps isolating the
# #9979/#9992 resolver-ordering invariant on its own, independent of #10077.

using Test

module Pkg9992Combine
export transform9992
transform9992(x) = x + 1
end

using .Pkg9992Combine

# Broad vararg fallback (least specific): the only match for a 3-arg call, and
# always the LEAST specific candidate whenever a fixed-arity method also
# matches.
combine9992(args...) = :vararg_fallback

# Same-arity `Any`/`Any` candidate: more specific than the vararg catch-all,
# but less specific than a `Function`-typed second parameter.
combine9992(label::Any, f::Any) = :any_any

# `Function`-typed candidate: most specific for a callable-value 2nd argument
# — this is the method a Function-singleton value from a module/package
# context must select over both distractors above (Issue #9979/#9992).
combine9992(label::String, f::Function) = :typed_function

@testset "callable value + module context + vararg fallback dispatch (Issue #9992)" begin
    # Function singleton imported from a user module (package-like context)
    # via its bare exported name (Issue #10077 sidestep — see file header).
    h = transform9992

    @test combine9992("via_module", h) == :typed_function
    @test combine9992(1, 2, 3) == :vararg_fallback
    @test combine9992("plain", 42) == :any_any

    # The captured module Function singleton still dispatches correctly
    # through ordinary calls and HOFs.
    @test h(41) == 42
    @test map(h, [1, 2, 3]) == [2, 3, 4]
end

true  # Test passed
