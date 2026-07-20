using Test

# Issue #10984 regression guard: `let` blocks already shadow/restore
# same-named outer locals correctly (`Expr::LetBlock` handling in
# `compile/expr/mod.rs`, the pre-existing idiom `shadow_local_enter`/
# `shadow_local_exit` on `CoreCompiler` generalizes for `for`/`foreach`/
# comprehension). Pin the `let` behavior so a future refactor of the shared
# shadow mechanism cannot regress it. Verified against
# `julia --startup-file=no` (1.12.6): prints `outer`.
function let_shadow()
    i = "outer"
    let i = 99
        i = i + 1
    end
    return i
end

@test let_shadow() == "outer"

true
