# Differentiation for the Symbolics subset (Issue #6572).
#
# A reduced port of `extern/Symbolics.jl/src/diff.jl`. Upstream `Differential(x)`
# builds a *lazy* `Term(D, [expr])` that `expand_derivatives` later evaluates via
# `executediff`. The subset is **eager**: `Differential(x)(expr)` computes the
# derivative immediately through `_deriv`, applying the sum/product/quotient/
# power/chain rules and a small table of elementary-function derivatives. Results
# are normalized through the shallow `_mk*` constructors, so e.g.
# `derivative(x^2, x)` collapses straight to `2x` and `Differential(x)(sin(x))`
# to `cos(x)`.
#
# Each rule lives in its own small helper rather than inline in `_deriv`: a
# single `_deriv` body containing every rule made the VM's load-time inference
# blow up (the `using Symbolics` loader hung). Splitting keeps each function
# small enough to infer (Issue #7186). That hang's root cause — an unbounded
# re-analysis in PartialStruct-return inference — is now fixed (negative caching
# of the constructor-partial result), so the general (x-dependent) power rule is
# supported again via `_deriv_genpow` (`(a^b)' = a^b·(b'·log a + b·a'/a)`).

"""
    Differential(x)

The derivative operator with respect to the variable `x` (a `Num` wrapping a
`Sym`). Applying it differentiates eagerly:

```julia
@variables x
D = Differential(x)
D(sin(x))   # cos(x)
D(x^2)      # 2x
```

In this subset `Differential(x)` returns a one-argument function (a closure), not
a struct instance: the VM does not dispatch a struct call operator
`(D::T)(args)` defined inside a module (Issue #7185). The closure must reference
only locals — a closure cannot resolve a module-level helper by name
(Issue #7180) — so the actual work is done by the regular function `_apply_diff`,
captured into a local before the closure is built.
"""
function Differential(x)
    xvar = unwrap(x)
    apply = _apply_diff
    return expr -> apply(expr, xvar)
end

# Eager derivative application as a regular module function (so it can freely
# call the module helpers `Num`/`unwrap`/`_deriv`).
_apply_diff(expr, xvar) = Num(_deriv(unwrap(expr), xvar))

# Does the symbol named `xname` occur anywhere in `node`?
function _occursin_var(xname, node)::Bool
    if node isa Sym
        return node.name === xname
    elseif node isa Term
        for a in arguments(node)
            _occursin_var(xname, a) && return true
        end
        return false
    else
        return false
    end
end

# Derivative of an elementary function with respect to its argument `a`.
function _elem_deriv(op::Symbol, a)::Any
    if op === :sin
        _applyelem(:cos, a)
    elseif op === :cos
        _mkneg(_applyelem(:sin, a))
    elseif op === :tan
        _mkdiv(1, _mkpow(_applyelem(:cos, a), 2))   # sec(a)^2
    elseif op === :exp
        _applyelem(:exp, a)
    elseif op === :log
        _mkdiv(1, a)
    elseif op === :sqrt
        _mkdiv(1, _mkmul(2, _applyelem(:sqrt, a)))
    else
        0
    end
end

# ── Per-rule helpers (kept small to keep load-time inference tractable) ──────
_deriv_add(a, b, x) = _mkadd(_deriv(a, x), _deriv(b, x))
_deriv_sub(a, b, x) = _mksub(_deriv(a, x), _deriv(b, x))

# product rule: (a*b)' = a'b + ab'
_deriv_mul(a, b, x) = _mkadd(_mkmul(_deriv(a, x), b), _mkmul(a, _deriv(b, x)))

# quotient rule: (a/b)' = (a'b - ab') / b^2
function _deriv_div(a, b, x)::Any
    num = _mksub(_mkmul(_deriv(a, x), b), _mkmul(a, _deriv(b, x)))
    _mkdiv(num, _mkpow(b, 2))
end

# power rule for an exponent that does not depend on `x`:
# (a^b)' = b * a^(b-1) * a'
function _deriv_pow(a, b, x)::Any
    bm1 = b isa Number ? b - 1 : _mksub(b, 1)
    _mkmul(_mkmul(b, _mkpow(a, bm1)), _deriv(a, x))
end

# general (x-dependent exponent) power rule, e.g. `x^x`, `2^x`:
#   (a^b)' = a^b * (b'·log(a) + b·a'/a)
# obtained by logarithmic differentiation of `a^b`. The nested `log` formula
# that mixes several `_mk*` helpers and recurses on the *second* argument (`b`)
# is exactly the shape that used to blow up load-time inference until the
# PartialStruct-return negative cache landed (Issue #7186); it is safe to
# evaluate eagerly now.
function _deriv_genpow(a, b, x)::Any
    da = _deriv(a, x)
    db = _deriv(b, x)
    inner = _mkadd(_mkmul(db, _applyelem(:log, a)), _mkmul(b, _mkdiv(da, a)))
    _mkmul(_mkpow(a, b), inner)
end

# chain rule: f(a)' = f'(a) * a'
_deriv_chain(op, a, x) = _mkmul(_elem_deriv(op, a), _deriv(a, x))

# Differentiate a bare node (Number / Sym / Term) with respect to the `Sym` `x`.
#
# The `::Any` return annotation is load-bearing for compile speed (Issue #7215),
# not a no-op: `_deriv` is the hub of a mutually recursive family
# (`_deriv ⇄ _deriv_add/_deriv_mul/…`). Without a declared return type the VM's
# abstract-interpretation engine re-infers this body at every call site inside
# the cycle, and because tentative cycle results are evicted each outer fixpoint
# iteration the same work repeats `depth × iterations × branching` times — ~7–17 s
# of `using Symbolics`/first-derivative compile. A declared return type lets the
# engine short-circuit the call site to that type instead of expanding the body.
# `Any` is exact-and-honest here (`_deriv` returns `0`/`1::Int` or a `Num`/`Term`)
# and triggers no runtime `convert` (`convert(Any, x) === x`); `_apply_diff` still
# infers to `Num` because it wraps the result in `Num(_deriv(…))`.
function _deriv(node, x)::Any
    if node isa Number
        return 0
    elseif node isa Sym
        return node.name === x.name ? 1 : 0
    elseif node isa Term
        op = operation(node)
        args = arguments(node)
        if op === :+
            return _deriv_add(args[1], args[2], x)
        elseif op === :- && length(args) == 2
            return _deriv_sub(args[1], args[2], x)
        elseif op === :- && length(args) == 1
            return _mkneg(_deriv(args[1], x))
        elseif op === :*
            return _deriv_mul(args[1], args[2], x)
        elseif op === :/
            return _deriv_div(args[1], args[2], x)
        elseif op === :^
            b = args[2]
            if b isa Number || !_occursin_var(x.name, b)
                return _deriv_pow(args[1], b, x)
            else
                # x-dependent exponent (e.g. 2^x, x^x): logarithmic
                # differentiation (Issue #7186 — this branch hung the loader
                # before the PartialStruct-return negative cache).
                return _deriv_genpow(args[1], b, x)
            end
        elseif _iselementary(op)
            return _deriv_chain(op, args[1], x)
        else
            return 0
        end
    else
        return 0
    end
end

"""
    derivative(expr, var)

Differentiate `expr` with respect to `var` (a `Num` variable), returning a `Num`.

```julia
@variables x
derivative(x^2 + sin(x), x)   # 2x + cos(x)
```
"""
derivative(expr, var)::Num = _apply_diff(expr, unwrap(var))

"""
    expand_derivatives(x)

Expand any unevaluated derivatives in `x`. In this subset `Differential` is
eager, so derivatives are already evaluated and this returns `x` unchanged; it
exists for API compatibility with upstream Symbolics.
"""
expand_derivatives(x) = x
