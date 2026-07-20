using Test

# Issue #8647: `eval` over a runtime-constructed `Expr` must accept a
# short-form method definition (`Expr(:(=), call_or_where, body)`), matching
# upstream `Meta.lower`'s treatment of a call-shaped assignment target as a
# method definition rather than a variable binding. Previously this raised
# "Type error: assignment target must be Symbol, got Expr".
#
# Scope covered here: brand-new names, multi-arg, `where` (accepted
# syntactically), trailing varargs, zero-arg, and redefinition observed by
# later `eval`-driven calls (including after the dispatch decision has been
# warmed by many prior calls — the Issue #8561 cache-invalidation contract
# extended to this path). Typed/keyword parameters and qualified/parametric
# names are deferred (raise a clear `Feature not implemented` error rather
# than mis-dispatching) — see the `eval_expr_function_def_deferred_8647.jl`
# fixture. A call site the AOT compiler already resolved/inlined/specialized
# *before* the `eval` ran cannot observe a redefinition at all: a separate,
# structural limitation tracked in Issue #8825.

@testset "eval of quoted short-form function definition (Issue #8647)" begin
    # Basic short-form definition of a brand-new name; `eval` of a method
    # definition returns the generic Function, matching upstream.
    @test eval(:(f8647a(x) = x + 1)) isa Function
    @test eval(:(f8647a(5))) == 6

    # Multi-arg.
    eval(:(f8647b(x, y) = x * y + 1))
    @test eval(:(f8647b(3, 4))) == 13

    # `where` clause: accepted syntactically (a bound-but-unused type
    # variable is legal upstream, if noisy).
    eval(:(f8647c(x) where {T} = x + 2))
    @test eval(:(f8647c(10))) == 12

    # Trailing varargs collect into a Tuple, matching upstream.
    eval(:(f8647d(x, ys...) = x + length(ys)))
    @test eval(:(f8647d(10, 1, 2, 3))) == 13

    # Zero-arg definition.
    eval(:(f8647e() = 42))
    @test eval(:(f8647e())) == 42

    # Redefinition through eval is observed by later eval-driven calls, even
    # after the dispatch decision for that name has been warmed by many
    # prior calls (Issue #8561 contract: `activate_eval_function` must flush
    # stale dispatch/inline caches on this path too, not only for `@eval`).
    eval(:(f8647f(x) = x + 1))
    warm_total = 0
    for _ in 1:20
        warm_total += eval(:(f8647f(1)))
    end
    @test warm_total == 40
    eval(:(f8647f(x) = x + 100))
    @test eval(:(f8647f(1))) == 101
end

true
