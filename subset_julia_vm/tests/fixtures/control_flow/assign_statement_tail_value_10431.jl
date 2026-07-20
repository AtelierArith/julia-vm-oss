# Issue #10431 (general fix; discovered while closing Issue #10254): an
# assignment IS an expression whose value is the assigned RHS value — Julia
# applies this to EVERY assignment-statement shape, not just a plain
# identifier assignment (`x = v` / `x += v` / `global x = v`, fixed by Issue
# #10074). `v[i] = x` (indexed), `obj.field = x` (field), `d[k] = x` (dict),
# and (partially — see below) `(a, b) = t` (tuple destructuring) must also
# evaluate to their RHS value when they sit in an implicit-return/tail-value
# position: a plain function-body tail, an `if`/`else` branch tail, a
# `begin ... end` block tail, or a `try`/`catch` branch tail.
#
# Root cause: `assign_block_tail_value` (lowering/expr/mod.rs, shared by
# try/if-as-value) and the compile-layer implicit-return paths
# (`compile_function_body`/`compile_block_with_implicit_return` in
# compile/stmt.rs, `compile_block_value` in compile/expr/mod.rs) only had
# arms for `Stmt::Assign`/`Stmt::AddAssign` (Issue #10074/#8976/#10023).
# `Stmt::IndexAssign`/`Stmt::FieldAssign`/`Stmt::DictAssign` fell through to
# the default "no value" case, silently producing `nothing` instead of the
# assigned value. Fix: bind the RHS to a fresh compiler-internal temporary,
# perform the store using the temporary (so the RHS expression and any index
# expressions are evaluated exactly once, matching Julia's evaluation order
# and avoiding a double side effect), and yield the temporary — mirroring
# the pattern already used for expression-position compound assignment
# (`compound_assign_stmt_to_value_expr`, Issue #7269/#8007).
#
# `x += v`-style compound assignment on an indexed/field target (`v[i] += 9`)
# already desugars at STATEMENT-lowering time to a plain `Stmt::IndexAssign`/
# `Stmt::FieldAssign` whose `value` is `v[i] + 9` — so it needs no separate
# handling once the plain-assign arms exist.
#
# Tuple destructuring (`(a, b) = t`) is more structurally involved: ordinary
# statement-level destructuring NEVER produces the dedicated
# `Stmt::DestructuringAssign` IR variant — `lower_tuple_destructuring_impl`
# always decomposes it into a flat `Stmt::Block` of per-target `Stmt::Assign`
# statements. When the RHS is not a literal tuple (or the targets read each
# other, e.g. a swap), the decomposition routes the RHS through one or more
# compiler-internal `__tuple_tmp_N`-prefixed temporaries; this fixture
# recognizes that reserved naming convention (mirroring the `__sjvm_try_result_`
# convention already used elsewhere in this file) to recover the destructured
# tuple's value as the block's tail value. The remaining sub-case — an
# INDEPENDENT literal-tuple RHS with matching arity (`(a, b) = (1, 2)`
# exactly) — uses a temp-free fast path (Issue #6569, avoids allocating the
# tuple) that is structurally indistinguishable from a genuine two-statement
# `begin; a = 1; b = 2; end` block once lowered, and is intentionally NOT
# covered by this fixture; it remains tracked as a known follow-up gap.

using Test

mutable struct AssignTailP
    x::Int
end

# ── IndexAssign: plain function tail ────────────────────────────────────

function idx_plain_tail()
    v = [0, 0]
    v[1] = 9
end

@testset "indexed assign, plain function tail (Issue #10431)" begin
    @test idx_plain_tail() == 9
end

# ── IndexAssign: if/else tail ───────────────────────────────────────────

function idx_if_tail(c)
    v = [0, 0]
    if c
        v[1] = 9
    else
        v[1] = -1
    end
end

@testset "indexed assign, if/else tail (Issue #10431)" begin
    @test idx_if_tail(true) == 9
    @test idx_if_tail(false) == -1
end

# ── IndexAssign: begin/end tail ─────────────────────────────────────────

function idx_block_tail()
    v = [0, 0]
    begin
        v[1] = 9
    end
end

@testset "indexed assign, begin/end tail (Issue #10431)" begin
    @test idx_block_tail() == 9
end

# ── IndexAssign: try/catch tail ─────────────────────────────────────────

function idx_try_tail()
    v = [0, 0]
    try
        v[1] = 9
    catch
        v[1] = -1
    end
end

@testset "indexed assign, try/catch tail (Issue #10431)" begin
    @test idx_try_tail() == 9
end

# ── IndexAssign op-assign (`+=`): plain function tail ───────────────────

function idx_addassign_plain_tail()
    v = [1, 1]
    v[1] += 9
end

@testset "indexed op-assign (+=), plain function tail (Issue #10431)" begin
    @test idx_addassign_plain_tail() == 10
end

# ── FieldAssign: plain function tail ────────────────────────────────────

function field_plain_tail()
    p = AssignTailP(0)
    p.x = 9
end

@testset "field assign, plain function tail (Issue #10431)" begin
    @test field_plain_tail() == 9
end

# ── FieldAssign: if/else tail ───────────────────────────────────────────

function field_if_tail(c)
    p = AssignTailP(0)
    if c
        p.x = 9
    else
        p.x = -1
    end
end

@testset "field assign, if/else tail (Issue #10431)" begin
    @test field_if_tail(true) == 9
    @test field_if_tail(false) == -1
end

# ── FieldAssign: begin/end tail ─────────────────────────────────────────

function field_block_tail()
    p = AssignTailP(0)
    begin
        p.x = 9
    end
end

@testset "field assign, begin/end tail (Issue #10431)" begin
    @test field_block_tail() == 9
end

# ── FieldAssign: try/catch tail ─────────────────────────────────────────

function field_try_tail()
    p = AssignTailP(0)
    try
        p.x = 9
    catch
        p.x = -1
    end
end

@testset "field assign, try/catch tail (Issue #10431)" begin
    @test field_try_tail() == 9
end

# ── FieldAssign op-assign (`+=`): plain function tail ───────────────────

function field_addassign_plain_tail()
    q = AssignTailP(1)
    q.x += 9
end

@testset "field op-assign (+=), plain function tail (Issue #10431)" begin
    @test field_addassign_plain_tail() == 10
end

# ── Dict indexed assign (lowers through the same IndexAssign shape): ───
#    plain tail, if/else tail, begin/end tail, try/catch tail.

function dict_plain_tail()
    d = Dict{Int,Int}()
    d[1] = 9
end

@testset "dict assign, plain function tail (Issue #10431)" begin
    @test dict_plain_tail() == 9
end

function dict_if_tail(c)
    d = Dict{Int,Int}()
    if c
        d[1] = 9
    else
        d[1] = -1
    end
end

@testset "dict assign, if/else tail (Issue #10431)" begin
    @test dict_if_tail(true) == 9
    @test dict_if_tail(false) == -1
end

function dict_block_tail()
    d = Dict{Int,Int}()
    begin
        d[1] = 9
    end
end

@testset "dict assign, begin/end tail (Issue #10431)" begin
    @test dict_block_tail() == 9
end

function dict_try_tail()
    d = Dict{Int,Int}()
    try
        d[1] = 9
    catch
        d[1] = -1
    end
end

@testset "dict assign, try/catch tail (Issue #10431)" begin
    @test dict_try_tail() == 9
end

# ── Typed post-use: caller performs arithmetic / typeof() on the ────────
#    assign-tail result, exercising return-type inference too.

function idx_typed_post_use()
    v = [0, 0]
    v[1] = 9
end

@testset "indexed assign tail, typed arithmetic post-use (Issue #10431)" begin
    @test idx_typed_post_use() + 1 == 10
    @test typeof(idx_typed_post_use()) == Int64
end

# ── DestructuringAssign: non-literal flat RHS (explicit IR as of #10464).

function destructure_nonliteral_tail()
    f() = (1, 2)
    (a, b) = f()
end

@testset "tuple destructuring assign, non-literal RHS, plain tail (Issue #10431)" begin
    @test destructure_nonliteral_tail() == (1, 2)
end

function destructure_nonliteral_try_tail()
    f() = (1, 2)
    try
        (a, b) = f()
    catch
        (a, b) = (-1, -1)
    end
end

@testset "tuple destructuring assign, non-literal RHS, try/catch tail (Issue #10431)" begin
    @test destructure_nonliteral_try_tail() == (1, 2)
end

# ── DestructuringAssign: dependent swap (routes through per-element ─────
#    `__tuple_tmp_N` temps, no single combined-tuple temp).

function destructure_swap_tail()
    a = 1
    b = 2
    (a, b) = (b, a)
end

@testset "tuple destructuring assign, dependent swap, plain tail (Issue #10431)" begin
    @test destructure_swap_tail() == (2, 1)
end

# ── Regression guard: a GENUINE nested `begin ... end` with two plain ────
#    assignments must still return the LAST statement's value, not get
#    misidentified as a destructuring decomposition and reconstructed into
#    a tuple. This is the negative case that proves the
#    `destructuring_tail_value` detection (keyed on the reserved
#    `__tuple_tmp_`-prefixed temp name, never on block shape or span) cannot
#    misfire on ordinary user code — including macro-expanded code, where
#    every statement in the block can share the macro-call's span.

function real_nested_block_two_assigns()
    begin
        a = 1
        b = 2
    end
end

@testset "real nested begin/end (two plain assigns) still returns last value, not a tuple (Issue #10431 regression guard)" begin
    @test real_nested_block_two_assigns() == 2
end

function real_nested_block_two_assigns_via_eval()
    @eval begin
        a = 1
        b = 2
    end
end

@testset "macro/@eval-expanded begin/end (two plain assigns) still returns last value (Issue #10431 regression guard)" begin
    @test real_nested_block_two_assigns_via_eval() == 2
end

true
