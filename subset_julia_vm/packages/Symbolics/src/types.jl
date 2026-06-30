# Core symbolic types for the Symbolics subset (Issue #6572).
#
# A faithful but heavily simplified port of the upstream type classification
# (`Num <: Real` wrapping `Sym`/`Term`), see `extern/Symbolics.jl/src/num.jl`
# and `extern/SymbolicUtils.jl/src/types.jl`. Upstream uses a hashconsed
# uni-type `BasicSymbolic` with seven variants (`Const`/`Sym`/`Term`/`AddMul`/
# `Div`/`ArrayOp`/`ArrayMaker`); that is impractical to port to the no-JIT VM,
# so the subset uses two plain structs (`Sym`, `Term`) plus the `Num` wrapper.

"""
    Sym(name)

A symbolic variable, e.g. the `x` created by `@variables x`. `name` holds a
`Symbol`.

The field is intentionally left untyped (rather than `name::Symbol`): the
`@variables` macro constructs `Sym` by splicing a `QuoteNode` into the generated
AST, and a macro-injected symbol literal is currently boxed as `Any` by the VM
(Issue #7163), so a `::Symbol`-typed field rejects it with "Cannot convert Any
to Symbol". The stored value is still a genuine `Symbol` (`Sym(:x).name === :x`).
"""
struct Sym
    name
end

"""
    Term(op::Symbol, args::Vector{Any})

A symbolic expression `op(args...)`. For example `x + 1` is
`Term(:+, Any[Sym(:x), 1])` and `sin(x)` is `Term(:sin, Any[Sym(:x)])`. `op` is
the operator/function as a `Symbol`; `args` may hold `Number`s, `Sym`s and
nested `Term`s.
"""
struct Term
    op::Symbol
    args::Vector{Any}
end

"""
    Num(val) <: Real

Wrap anything in a type that is a subtype of `Real`, mirroring upstream
`Num <: Real`. `val` is a `Real`, `Sym` or `Term`.
"""
struct Num <: Real
    val::Any
end

# Idempotent wrapping, mirrors upstream `Num(x::Num) = x`.
Num(x::Num) = x

"""
    unwrap(x)

Return the value wrapped by a `Num`, or `x` itself for anything else.
"""
unwrap(x::Num) = x.val
unwrap(x) = x

"""
    value(x)

Alias for [`unwrap`](@ref): peel a `Num` wrapper, leaving everything else as-is.
"""
value(x) = unwrap(x)

# TermInterface-style accessors — the public way to inspect a `Term`, mirroring
# upstream `operation`/`arguments`/`iscall`.
#
# Always prefer these over direct field access: `t.args` on a dynamically-typed
# (`Any`) value mis-routes to the builtin `Expr.args` accessor and errors
# ("GetExprField: expected Expr, got StructRef"). These dispatch on `::Term`,
# which narrows the type so the field resolves correctly.

"""
    operation(t::Term)

The operator/function symbol of a `Term`, e.g. `operation(x + 1) === :+`.
"""
operation(t::Term) = t.op

"""
    arguments(t::Term)

The argument vector of a `Term`, e.g. `arguments(sin(x)) == Any[Sym(:x)]`.
"""
arguments(t::Term) = t.args

"""
    iscall(x)

`true` if `x` is a composite expression (`Term`), `false` for `Sym`/numbers.
"""
iscall(x::Term) = true
iscall(x) = false

"""
    issym(x)

`true` if `x` is a symbolic variable (`Sym`).
"""
issym(x::Sym) = true
issym(x) = false

"""
    isterm(x)

`true` if `x` is a `Term`.
"""
isterm(x::Term) = true
isterm(x) = false
