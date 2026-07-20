# Regression test for Issue #11088: same-named functions declared in sibling
# modules must NOT compare `==`/`===` as one value, and must not share a
# `typeof`. This is the function/method-domain analog of Issue #11021 (same-
# named structs in sibling modules colliding under `==`/`===`), found while
# investigating Issue #10990 (Phase 3 of the semantic-ID epic #10459,
# `FunctionId`/`MethodId`).
#
# `emit_function_value_named` (subset_julia_vm_compile/src/compile/core_compiler.rs)
# always baked the BARE declared name into a resolved function value's
# runtime type identity -- a correct fix for Issue #10077 (the SAME
# declaration must report the SAME `typeof` regardless of whether it is
# reached via a qualified `Module.func` or a bare/imported `func`), but it did
# not distinguish that case from two DIFFERENT declarations that merely
# happen to share a bare name across sibling modules.
using Test

module F1x11088
f(x) = "F1x_f"
end

module F2x11088
f(x) = "F2x_f"
end

# Nested (nested-module) same-named functions.
module FOuter1x11088
module FInner1x11088
g() = "FOuter1x_FInner1x_g"
end
end

module FOuter2x11088
module FInner2x11088
g() = "FOuter2x_FInner2x_g"
end
end

# Issue #10077's own invariant must still hold: a function extended via
# selective import shares ONE identity across both its bare (imported) and
# module-qualified access paths.
module F3x11088
export h
h(x) = "F3x_h"
end
using .F3x11088

# Adversarial-review regression guard: an UNRELATED sibling module that
# declares the SAME bare name as an already-`using`d module, but is itself
# never `using`d, must NOT flip the `using`d declaration's bare-vs-qualified
# identity apart (a naive "does any other qualified key exist" check does
# exactly that, since every module-scoped function is registered into the
# shared bare-name method table regardless of `using`, Issue #11089's own
# root cause). `F4x11088.h` is never `using`d anywhere in this file. Uses a
# DIFFERENT arity than `F3x11088.h(x)` so this fixture stays isolated from
# Issue #11089's separate CALLING-dispatch bug (a same-arity/same-signature
# unrelated sibling can, via the shared bare-name table's dedup, steal a
# `using`d declaration's bare CALL target too -- that is #11089's own
# territory, not this fixture's identity-only scope).
module F4x11088
h(x, y) = "F4x_h"
end

# Second adversarial-review regression guard (round 2): a module that IS
# itself `using`d, but only for a DIFFERENT name, and privately (non-
# exported) happens to also define the SAME bare name as another `using`d
# module's EXPORTED declaration, must NOT be treated as an ambiguous or
# competing owner either -- `using` only brings a module's *exported* names
# into unqualified scope, so an un-exported same-named declaration in a
# module that is itself `using`d (just for something else) is still
# invisible to bare lookup. Uses a DIFFERENT arity than `F5x11088.k(x)` for
# the same isolation-from-#11089-calling-dispatch reason as `F4x11088.h`
# above.
module F5x11088
export k
k(x) = "F5x_k"
end
module F6x11088
export m
k(x, y) = "F6x_k"  # NOT exported -- must not shadow F5x11088's exported k
m(x) = "F6x_m"
end
using .F5x11088
using .F6x11088

@testset "same-named function identity across sibling modules (Issue #11088)" begin
    # Different declarations sharing a bare name must be distinct values.
    @test (F1x11088.f === F2x11088.f) == false
    @test (F1x11088.f == F2x11088.f) == false
    @test (typeof(F1x11088.f) === typeof(F2x11088.f)) == false
    @test (typeof(F1x11088.f) == typeof(F2x11088.f)) == false

    # A function is still identical to itself (sanity: the fix must not make
    # every function comparison false).
    @test F1x11088.f === F1x11088.f
    @test typeof(F1x11088.f) === typeof(F1x11088.f)

    # Calling correctness is unaffected by the identity fix.
    @test F1x11088.f(1) == "F1x_f"
    @test F2x11088.f(1) == "F2x_f"

    # Nested modules: same base function name at the same nesting depth
    # under different parents must stay distinct.
    @test (typeof(FOuter1x11088.FInner1x11088.g) === typeof(FOuter2x11088.FInner2x11088.g)) == false
    @test FOuter1x11088.FInner1x11088.g() == "FOuter1x_FInner1x_g"
    @test FOuter2x11088.FInner2x11088.g() == "FOuter2x_FInner2x_g"

    # Issue #10077 invariant: the SAME declaration accessed bare (after
    # `using`) vs. module-qualified must still report the SAME typeof/`===`,
    # EVEN THOUGH an unrelated, never-`using`d sibling (F4x11088.h) also
    # declares a function with the same bare name `h` (the adversarial-review
    # regression this fixture guards -- see the module-level comment above
    # `module F4x11088`).
    h_bare = h
    h_qualified = F3x11088.h
    @test h_bare === h_qualified
    @test typeof(h_bare) === typeof(h_qualified)
    @test h_bare(1) == h_qualified(1)

    # Round-2 adversarial-review regression guard: `F6x11088.k` is a private
    # (non-exported) same-named declaration in a module that IS `using`d
    # (for `m`), which must not shadow `F5x11088`'s EXPORTED `k` either --
    # see the module-level comment above `module F5x11088`.
    k_bare = k
    k_qualified = F5x11088.k
    @test k_bare === k_qualified
    @test typeof(k_bare) === typeof(k_qualified)
    @test k_bare(1) == k_qualified(1)
    @test m(1) == "F6x_m"
end

true
