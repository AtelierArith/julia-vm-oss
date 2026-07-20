# Regression test for Issue #11176, found while auditing `module_aliases`
# for Issue #11032 (techdebt(#10459): Phase 2a continuation — ModuleId
# migration scope judgment for the 12 named module/global tables).
#
# When two different `using` imports bring a same-named submodule into scope
# from two DIFFERENT parent modules (`using .A: Sub` then `using .B: Sub`,
# both `A` and `B` declaring their own `Sub`), upstream Julia keeps the FIRST
# import and warns about the conflicting second one ("ignoring conflicting
# import of ... into ..."). `imported_submodule_aliases`
# (`subset_julia_vm_compile/src/compile/core_compiler.rs`) used to iterate an
# UNORDERED `usings: &HashSet<String>` and unconditionally overwrite the bare
# alias on every match, so the observed "winner" depended on `HashSet`
# iteration order rather than source order — an unrelated same-bare-name
# submodule from a LATER `using` could silently clobber an earlier one. Fixed
# by iterating the already-available, source-ordered `resolved_usings` and
# keeping only the FIRST alias assignment per bare name (`entry(..)
# .or_insert(..)`, not `insert`).
#
# Two independent scopes below exercise BOTH orderings of the same two
# conflicting submodules, so the pinned behavior is provably driven by
# SOURCE ORDER (first `using` wins) rather than by name or by which module
# happens to compile/hash first.
using Test

module Scope1_11176
module Alpha1176
    module Sub
        greet() = "Alpha"
    end
    export Sub
end
module Beta1176
    module Sub
        greet() = "Beta"
    end
    export Sub
end
using .Alpha1176: Sub
using .Beta1176: Sub
getresult() = Sub.greet()
end

module Scope2_11176
module Alpha2176
    module Sub
        greet() = "Alpha"
    end
    export Sub
end
module Beta2176
    module Sub
        greet() = "Beta"
    end
    export Sub
end
using .Beta2176: Sub
using .Alpha2176: Sub
getresult() = Sub.greet()
end

@testset "submodule alias first-using-wins (Issue #11176)" begin
    # Scope1: Alpha1176's Sub is imported FIRST -> it wins.
    @test Scope1_11176.getresult() == "Alpha"
    # Scope2: same conflict, opposite source order -> Beta2176's Sub wins.
    # This half of the pin is what actually rules out "alphabetical" or
    # "hash-order" explanations for the Scope1 result above.
    @test Scope2_11176.getresult() == "Beta"
end

true
