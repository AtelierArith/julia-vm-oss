# Issue #10214: `function_module_paths` (Issue #7180) and
# `module_scope_overrides` (Issue #10073) were both keyed by BARE function
# name in `subset_julia_vm_compile/src/compile/pipeline_ctx.rs`/`collect.rs`. When two
# different modules define a function/closure with the same bare name,
# whichever module was processed last won in the map, so the OTHER module's
# same-named helper silently resolved against the WRONG module's globals —
# wrong output, not a crash.
#
# Fixed by keying module-scope resolution by a fully-qualified identity
# instead of bare name:
#   - `function_module_paths` (top-level module functions, e.g. `outer`):
#     keyed by `"Module.path.func_name"`. `collect_from_module` also gives a
#     module's nested/closure functions a MODULE-QUALIFIED parent identity
#     (`"Module.path.outer"`, not bare `"outer"`) so their qualified
#     `"parent#child"` name (used by `function_indices`/`closure_captures`/
#     method-table registration) cannot collide with another module's
#     same-named parent's same-named child.
#   - `module_scope_overrides` (module-body `let`/`@testset`-root helpers):
#     keyed by the helper's collection INDEX in `inline_functions`, not bare
#     name.
#
# `MEma`/`MEmb` exercise the `function_module_paths` mechanism (both declare a
# top-level `outer()` with a same-named nested closure `helper` reading a
# module global `G`); `MTa`/`MTb` exercise `module_scope_overrides` (both
# declare a `@testset` with a same-named helper `f` reading a module global
# `G`).
module MEma
G = 1
function outer()
    helper(x) = x + G
    helper(0)
end
end

module MEmb
G = 2
function outer()
    helper(x) = x + G
    helper(0)
end
end

module MTa
using Test
G = 1
RESULT = Ref(0)
@testset "a" begin
    f(x) = x + G
    v = f(0)
    RESULT[] = v
    @test v == 1
end
end

module MTb
using Test
G = 2
RESULT = Ref(0)
@testset "b" begin
    f(x) = x + G
    v = f(0)
    RESULT[] = v
    @test v == 2
end
end

MEma.outer() == 1 &&
    MEmb.outer() == 2 &&
    MTa.RESULT[] == 1 &&
    MTb.RESULT[] == 2 &&
    true
