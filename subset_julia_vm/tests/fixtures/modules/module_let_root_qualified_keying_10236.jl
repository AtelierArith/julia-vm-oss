# Issue #10236 (follow-up of #10073/PR #10219/#10214): two adjacent
# module-body `let`-root helper collection/scope gaps.
#
# Gap 1: a STATEMENT-POSITION `let` inside a module body whose helper is
# called via an effect (a `Ref` store), not an assignment-RHS `let`, must
# still be collected and resolve module-scope globals. `MGap1` covers this
# (also exercised, incidentally, by the Issue #10227 generator-collection fix
# generalizing `collect_module_body_let_functions`'s statement handling to
# recurse through arbitrary exprs instead of matching only a literal
# `LetBlock` shape).
#
# Gap 2: same root cause as #10214 — `module_scope_overrides` used to be keyed
# by bare helper name, so a module-body `let`-root helper (`h`) collided with
# an UNRELATED same-named Main-level `let`-root helper (also `h`): the
# runtime `Value::Closure`/`Value::Function` created for the Main-level `h`
# could resolve to the module's `h`'s body (or vice versa), since both ended
# up registered under the same name somewhere in the lookup chain. Fixed by
# keying `module_scope_overrides` by the helper's collection index (not bare
# name) AND, for the METHOD-TABLE-alias layer, adding
# `FunctionInfo::suppress_short_name_alias` so a module-body `let`-root
# helper's qualified name (`"Module.path.h"`) does not ALSO expose it under
# the bare runtime name `"h"` (which a Main-level `let`-root helper of the
# same name legitimately owns). `MGap2` (paired with the Main-level `let`
# after it) covers this.
module MGap1
using Test
R = Ref(0)
let
    h(x) = x + 1
    R[] = h(1)
end
@testset "gap1 statement-position module-body let" begin
    @test R[] == 2
end
end

module MGap2
using Test
G = 10
R = Ref(0)
let
    h(x) = x + G
    R[] = h(1)
end
@testset "gap2 module-let helper sees its own module's global" begin
    @test R[] == 11
end
end

using Test
K = 100
gap2_main_let_result = let
    h(x) = x + K
    h(1)
end
@testset "gap2 main-level let helper does not collide with the module's same-named helper" begin
    @test gap2_main_let_result == 101
end

true
