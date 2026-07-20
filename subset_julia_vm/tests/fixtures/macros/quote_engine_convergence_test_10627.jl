# Issue #10627: differential test between the STATIC stdlib/Base
# quote-expansion engine (lowering/expr/quote/) and the DYNAMIC VM-backed
# macro-runtime engine (macro_runtime.rs).
#
# `Test.@test` (stdlib/Test/src/Test.jl) is a genuine STATIC-path macro (it
# is registered via STDLIB_MACROS / expand_stdlib_macro -> the
# lowering/expr/quote/ pipeline, unlike @time/@elapsed/@show which -- despite
# living in base/ -- are special-cased to the dynamic macro_runtime engine by
# `base_macro_preserves_statement_value`, Issue #7764; see also the
# newly-discovered Issue #10977 found while investigating that). `@test`'s
# quote body is `local threw/detail/result/recorded; try ... catch e ... end;
# if threw ... elseif result isa Bool; if result ... else ... end; else ...
# end` -- Block, Local (x4), Try/Catch, If, ElseIf, a nested If/Else, Call,
# and String, essentially the whole representative head matrix in one macro.
# Compile-time codegen (Pass 1 hygiene collection + Pass 2 IR construction)
# touches EVERY branch of that AST regardless of which one executes at
# runtime, so a single SAFE (passing) invocation already exercises the full
# static-engine head matrix.
#
# `@my_test` below mirrors that exact quote-body shape as a user-defined
# macro (dynamic path), using its own minimal recorder instead of Test's
# internal `_test_record!` builtins, so both engines are compared on
# textually-equivalent bodies -- both dispatch every one of these heads
# through the shared `quote_binding_role` classifier (`expr_heads.rs`).
#
# NOTE: this fixture deliberately invokes `Test.@test` ONLY with a PASSING
# condition. A genuinely failing/erroring `Test.@test` sets the sticky
# `Vm::any_test_failed()` flag the fixture harness's Issue #9360 gate
# enforces (`docs/vm/TESTSET_FAILURE_ALLOWLIST.tsv` explicitly forbids
# allowlisting new fixtures for this), and -- as newly discovered while
# writing this fixture (Issue #10978) -- a failing top-level `@testset` is
# not even catchable via `try`/`catch` in sjulia the way upstream's
# `FallbackTestSetException` is. `@my_test`'s classification of fail/throw/
# non-Boolean inputs is therefore checked standalone (dynamic engine only,
# self-consistent against the same logic `@test`'s real quote body
# implements), not against a real failing `Test.@test` invocation.
using Test

const MY_OUTCOMES = Int[]

function _my_record!(recorded::Int)
    push!(MY_OUTCOMES, recorded)
    recorded
end

macro my_test(ex)
    expr_str = string(ex)
    quote
        local threw = false
        local detail = ""
        local result = false
        local recorded = 0
        try
            result = $(esc(ex))
        catch e
            threw = true
            detail = string("Test threw exception: ", sprint(showerror, e))
        end
        if threw
            recorded = 2
        elseif result isa Bool
            if result
                recorded = 0
            else
                recorded = 1
            end
        else
            recorded = 2
        end
        _my_record!(recorded)
    end
end

# 0 = pass, 1 = fail, 2 = error (thrown or non-Boolean).
function static_class(r)
    r isa Test.Pass ? 0 : r isa Test.Fail ? 1 : 2
end

# Both engines classify a passing condition identically -- exercised via a
# REAL invocation of both macros (safe: `1 == 1` never fails, so the
# Issue #9360 testset-failure gate is never triggered).
r_static_pass = @test(1 == 1)
r_dynamic_pass = @my_test(1 == 1)
check_pass = static_class(r_static_pass) == 0 && r_dynamic_pass == 0

# The dynamic engine's own classification of fail/throw/non-Boolean inputs,
# using the exact same quote-body logic `@test`'s real (static-engine) body
# implements -- see the note above for why these are not also exercised via
# a real failing `Test.@test` call in this fixture.
check_dynamic_fail = @my_test(1 == 2) == 1
check_dynamic_throw = @my_test(error("boom")) == 2
check_dynamic_nonbool = @my_test(1 + 1) == 2

# Neither engine's own quote-internal locals (`threw`/`detail`/`result`/
# `recorded`) may leak into (or be visible to) top-level code outside the
# macro expansions above.
check_no_leak = !isdefined(Main, :threw) && !isdefined(Main, :detail) &&
    !isdefined(Main, :recorded)

check_pass && check_dynamic_fail && check_dynamic_throw && check_dynamic_nonbool &&
    check_no_leak
