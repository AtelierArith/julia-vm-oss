# Issue #10074: a `try/catch[/else/finally]` in tail (implicit-return)
# position whose branch ends in a plain assignment (`x = value`), an
# op-assignment (`x += value`), a `global x = value`, or a nested
# `begin ... end` block wrapping one of those returned the wrong value
# (silently `nothing`) or crashed with a spurious runtime `Type error`
# instead of the assigned value.
#
# Upstream Julia: an assignment is itself an expression whose value is the
# assigned value, and "the last statement of a block is its value" applies
# recursively through `try`/`catch`/`if`/`begin...end` branch tails.
#
# Root cause (two locations, both fixed together):
#   1. `assign_block_tail_value` (subset_julia_vm_lowering/src/lowering/expr/mod.rs),
#      shared by `try_stmt_into_value_expr` (Issue #6223) and
#      `if_stmt_into_value_expr`, rewrites a branch's trailing `Stmt::Expr`/
#      `Stmt::Try`/`Stmt::If`/`Stmt::Block` into an assignment to a fresh
#      result variable, but never recognized a trailing `Stmt::Assign`/
#      `Stmt::AddAssign` (the #8976/#10023 fix taught this to
#      `compile_function_body`/`compile_block_with_implicit_return`, but
#      never to this shared try/if-as-value helper).
#   2. `compile_block_value` (subset_julia_vm_compile/src/compile/expr/mod.rs), the
#      codegen for an empty-binding `Expr::LetBlock` (what a `begin ... end`
#      block lowers to), had the same gap: no arm for a trailing
#      `Stmt::Assign`/`Stmt::AddAssign`, so any `begin ... end` ending in a
#      plain assignment evaluated to `nothing` instead of the assigned
#      value — reachable directly (`x = begin y = 1 end`) and as a nested
#      tail block inside a `try`/`catch`/`if` branch.

using Test

global gtx1 = 0
global gtx2 = 0

# ── Plain assignment tail, try branch taken ─────────────────────────────

function try_tail_assign_try_branch()
    try
        x = 5
    catch
        x = -1
    end
end

@testset "try/catch tail plain-assign — try branch taken (Issue #10074)" begin
    @test try_tail_assign_try_branch() == 5
end

# ── Plain assignment tail, catch branch taken ───────────────────────────

function try_tail_assign_catch_branch()
    try
        error("boom")
        x = 5
    catch
        x = -1
    end
end

@testset "try/catch tail plain-assign — catch branch taken (Issue #10074)" begin
    @test try_tail_assign_catch_branch() == -1
end

# ── Op-assign tail (both branches) ──────────────────────────────────────

function try_tail_opassign()
    x = 10
    try
        x += 5
    catch
        x -= 1
    end
end

@testset "try/catch tail op-assign — try branch taken (Issue #10074)" begin
    @test try_tail_opassign() == 15
end

function try_tail_opassign_catch()
    x = 10
    try
        error("boom")
        x += 5
    catch
        x -= 1
    end
end

@testset "try/catch tail op-assign — catch branch taken (Issue #10074)" begin
    @test try_tail_opassign_catch() == 9
end

# ── global-assign tail: return value AND module binding ────────────────

function try_tail_global_assign_try_branch()
    try
        global gtx1 = 5
    catch
        global gtx1 = -1
    end
end

@testset "try/catch tail global-assign — try branch, return value (Issue #10074)" begin
    @test try_tail_global_assign_try_branch() == 5
end

@testset "try/catch tail global-assign — try branch, module binding (Issue #10074)" begin
    @test gtx1 == 5
end

function try_tail_global_assign_catch_branch()
    try
        error("boom")
        global gtx2 = 5
    catch
        global gtx2 = -1
    end
end

@testset "try/catch tail global-assign — catch branch, return value (Issue #10074)" begin
    @test try_tail_global_assign_catch_branch() == -1
end

@testset "try/catch tail global-assign — catch branch, module binding (Issue #10074)" begin
    @test gtx2 == -1
end

# ── else branch tail-assign replaces the try value ──────────────────────

function try_tail_else_assign()
    try
        x = 5
    catch
        x = -1
    else
        z = 99
    end
end

@testset "try/catch tail plain-assign — else branch replaces try value (Issue #10074)" begin
    @test try_tail_else_assign() == 99
end

# ── try/finally: finally's value must NOT become the return value ──────

function try_tail_assign_with_finally()
    try
        x = 5
    catch
        x = -1
    finally
        y = 999
    end
end

@testset "try/catch/finally tail plain-assign — finally does not override value (Issue #10074)" begin
    @test try_tail_assign_with_finally() == 5
end

function try_tail_assign_catch_with_finally()
    try
        error("boom")
        x = 5
    catch
        x = -1
    finally
        y = 999
    end
end

@testset "try/catch/finally tail plain-assign — catch branch, finally does not override (Issue #10074)" begin
    @test try_tail_assign_catch_with_finally() == -1
end

# ── Nested tail block (`begin ... end`) inside a try/catch branch ──────

function try_tail_nested_block_try_branch()
    try
        begin
            x = 42
        end
    catch
        x = -1
    end
end

@testset "try/catch tail nested begin/end block — try branch (Issue #10074)" begin
    @test try_tail_nested_block_try_branch() == 42
end

function try_tail_nested_block_catch_branch()
    try
        error("boom")
        x = 42
    catch
        begin
            x = -7
        end
    end
end

@testset "try/catch tail nested begin/end block — catch branch (Issue #10074)" begin
    @test try_tail_nested_block_catch_branch() == -7
end

# ── Mixed-type branches: value must be preserved regardless of which
#    branch's type "wins" ────────────────────────────────────────────────

function try_tail_mixed_type_no_throw()
    try
        x = 5
    catch
        x = "err"
    end
end

@testset "try/catch tail plain-assign — mixed types, try branch taken (Issue #10074)" begin
    @test try_tail_mixed_type_no_throw() == 5
end

function try_tail_mixed_type_throw()
    try
        error("boom")
        x = 5
    catch
        x = "err"
    end
end

@testset "try/catch tail plain-assign — mixed types, catch branch taken (Issue #10074)" begin
    @test try_tail_mixed_type_throw() == "err"
end

# ── A bare `begin ... end` ending in a plain assignment, in tail position
#    of a function body (no try/catch) and inside an `if` branch — same
#    root cause (`compile_block_value`), verified alongside the try/catch
#    coverage above.  ────────────────────────────────────────────────────

function plain_nested_block_tail_assign()
    begin
        x = 42
    end
end

@testset "bare begin/end block tail plain-assign (Issue #10074)" begin
    @test plain_nested_block_tail_assign() == 42
end

function if_nested_block_tail_assign(c)
    if c
        begin
            x = 42
        end
    else
        x = -1
    end
end

@testset "if branch with nested begin/end tail plain-assign (Issue #10074)" begin
    @test if_nested_block_tail_assign(true) == 42
    @test if_nested_block_tail_assign(false) == -1
end

true
