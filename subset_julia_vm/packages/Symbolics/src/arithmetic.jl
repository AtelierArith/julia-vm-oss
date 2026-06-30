# Arithmetic, elementary functions and equality for the Symbolics subset
# (Issue #6572).
#
# A reduced port of `extern/Symbolics.jl/src/num.jl`. Operators are overloaded on
# `Num` with the three mixed-type methods (`Num⊗Num`, `Num⊗Real`, `Real⊗Num`)
# upstream generates with `@number_methods`/`@num_method`. The mixed-type methods
# are MANDATORY: `Num <: Real`, so a missing `Num⊗Real`/`Real⊗Num` pair would
# fall to the generic promote-based fallback which can recurse forever — the
# trap documented in `docs/vm/PROMOTION.md` (Issue #5966). Every method here
# unwraps to plain `Real`/`Sym`/`Term`, normalizes, and re-wraps in `Num`, so a
# `Num` never reaches the `_mk*` constructors and the fallback is never hit.

# ── Shallow normalization constructors ──────────────────────────────────────
# Fold when both operands are numbers, drop additive/multiplicative identities,
# otherwise build a `Term`. Kept shallow on purpose to curb expression blow-up
# and deep recursion; deeper canonicalization is left to `simplify`/`expand`.

_iszeroc(x) = x isa Number && iszero(x)
_isonec(x) = x isa Number && isone(x)

function _mkadd(a, b)
    a isa Number && b isa Number && return a + b
    _iszeroc(a) && return b
    _iszeroc(b) && return a
    Term(:+, Any[a, b])
end

function _mkneg(a)
    a isa Number && return -a
    Term(:-, Any[a])
end

function _mksub(a, b)
    a isa Number && b isa Number && return a - b
    _iszeroc(b) && return a
    _iszeroc(a) && return _mkneg(b)
    Term(:-, Any[a, b])
end

function _mkmul(a, b)
    a isa Number && b isa Number && return a * b
    (_iszeroc(a) || _iszeroc(b)) && return 0
    _isonec(a) && return b
    _isonec(b) && return a
    Term(:*, Any[a, b])
end

function _mkdiv(a, b)
    a isa Number && b isa Number && return a / b
    _iszeroc(a) && return 0
    _isonec(b) && return a
    Term(:/, Any[a, b])
end

function _mkpow(a, b)
    a isa Number && b isa Number && return a^b
    _iszeroc(b) && return 1
    _isonec(b) && return a
    Term(:^, Any[a, b])
end

# Elementary function application on a bare node: fold a numeric argument,
# otherwise wrap a `Term`. Shared by the `Base.sin`/... methods below and by the
# `substitute`/`simplify`/`derivative` rebuild paths.
_iselementary(op::Symbol) =
    op === :sin || op === :cos || op === :tan || op === :exp || op === :log || op === :sqrt

function _applyelem(op::Symbol, a)
    if a isa Number
        op === :sin ? sin(a) :
        op === :cos ? cos(a) :
        op === :tan ? tan(a) :
        op === :exp ? exp(a) :
        op === :log ? log(a) :
        op === :sqrt ? sqrt(a) : Term(op, Any[a])
    else
        Term(op, Any[a])
    end
end

# Re-apply the shallow normalization for an operator and its (already-normalized)
# argument vector. Used to fold results after `substitute`/`simplify`/derivative
# rewrite a subtree. Unknown heads fall back to a plain `Term`.
function _rebuild(op::Symbol, args)
    if op === :+ && length(args) == 2
        _mkadd(args[1], args[2])
    elseif op === :- && length(args) == 2
        _mksub(args[1], args[2])
    elseif op === :- && length(args) == 1
        _mkneg(args[1])
    elseif op === :* && length(args) == 2
        _mkmul(args[1], args[2])
    elseif op === :/ && length(args) == 2
        _mkdiv(args[1], args[2])
    elseif op === :^ && length(args) == 2
        _mkpow(args[1], args[2])
    elseif _iselementary(op) && length(args) == 1
        _applyelem(op, args[1])
    else
        Term(op, Vector{Any}(args))
    end
end

# ── Operators (three mixed-type methods each + unary) ────────────────────────

Base.:+(a::Num, b::Num) = Num(_mkadd(unwrap(a), unwrap(b)))
Base.:+(a::Num, b::Real) = Num(_mkadd(unwrap(a), b))
Base.:+(a::Real, b::Num) = Num(_mkadd(a, unwrap(b)))
Base.:+(a::Num) = a

Base.:-(a::Num, b::Num) = Num(_mksub(unwrap(a), unwrap(b)))
Base.:-(a::Num, b::Real) = Num(_mksub(unwrap(a), b))
Base.:-(a::Real, b::Num) = Num(_mksub(a, unwrap(b)))
Base.:-(a::Num) = Num(_mkneg(unwrap(a)))

Base.:*(a::Num, b::Num) = Num(_mkmul(unwrap(a), unwrap(b)))
Base.:*(a::Num, b::Real) = Num(_mkmul(unwrap(a), b))
Base.:*(a::Real, b::Num) = Num(_mkmul(a, unwrap(b)))

Base.:/(a::Num, b::Num) = Num(_mkdiv(unwrap(a), unwrap(b)))
Base.:/(a::Num, b::Real) = Num(_mkdiv(unwrap(a), b))
Base.:/(a::Real, b::Num) = Num(_mkdiv(a, unwrap(b)))

Base.:^(a::Num, b::Num) = Num(_mkpow(unwrap(a), unwrap(b)))
Base.:^(a::Num, b::Real) = Num(_mkpow(unwrap(a), b))
# `Num ^ Integer` is defined separately to resolve the ambiguity with Base's
# `^(::Number, ::Integer)` (`x^2` lowers to `literal_pow` → `^(x, 2)`), mirroring
# upstream `Base.:^(n::Num, i::Integer)`.
Base.:^(a::Num, b::Integer) = Num(_mkpow(unwrap(a), b))
Base.:^(a::Real, b::Num) = Num(_mkpow(a, unwrap(b)))

# ── Elementary functions ─────────────────────────────────────────────────────
# A constant `Num` folds to the numeric result (via `_applyelem`); a symbolic
# argument wraps a `Term`. Each `op` is a literal `Symbol` written here in
# source (never macro injected), so a `::Symbol`-typed `Term.op` field is fine.

Base.sin(x::Num) = Num(_applyelem(:sin, unwrap(x)))
Base.cos(x::Num) = Num(_applyelem(:cos, unwrap(x)))
Base.tan(x::Num) = Num(_applyelem(:tan, unwrap(x)))
Base.exp(x::Num) = Num(_applyelem(:exp, unwrap(x)))
Base.log(x::Num) = Num(_applyelem(:log, unwrap(x)))
Base.sqrt(x::Num) = Num(_applyelem(:sqrt, unwrap(x)))

# ── Equality ─────────────────────────────────────────────────────────────────
# `_structeq` compares the symbolic trees structurally and returns a `Bool`.
# `==` folds to a numeric comparison when both sides are numbers and otherwise
# falls back to structural equality. (Subset divergence: upstream `==` on free
# symbols builds a symbolic comparison expression; the core set returns a `Bool`,
# which is what `substitute`/derivative parity checks need.) Mixed-type methods
# are again mandatory to avoid the promote-fallback trap (Issue #5966).

function _structeq(a, b)::Bool
    if a isa Sym && b isa Sym
        return a.name === b.name
    elseif a isa Term && b isa Term
        a.op === b.op || return false
        length(a.args) == length(b.args) || return false
        for i in eachindex(a.args)
            _structeq(a.args[i], b.args[i]) || return false
        end
        return true
    elseif a isa Number && b isa Number
        return a == b
    else
        return false
    end
end

_eqval(a, b) = (a isa Number && b isa Number) ? (a == b) : _structeq(a, b)

Base.:(==)(a::Num, b::Num) = _eqval(unwrap(a), unwrap(b))
Base.:(==)(a::Num, b::Real) = _eqval(unwrap(a), b)
Base.:(==)(a::Real, b::Num) = _eqval(a, unwrap(b))

Base.isequal(a::Num, b::Num) = _structeq(unwrap(a), unwrap(b))
Base.isequal(a::Num, b::Real) = _structeq(unwrap(a), b)
Base.isequal(a::Real, b::Num) = _structeq(a, unwrap(b))

# Structural hash, consistent with `==`/`isequal`, so a `Num` works as a `Dict`
# key (e.g. `substitute(ex, Dict(x => 3))`, `d[x]`). Mirrors upstream
# `hash(x::Num, h) = hash(unwrap(x), h)`.
function _symhash(x, h::UInt)
    if x isa Sym
        hash(x.name, h)
    elseif x isa Term
        h2 = hash(operation(x), h)
        for a in arguments(x)
            h2 = _symhash(a, h2)
        end
        h2
    else
        hash(x, h)
    end
end
Base.hash(x::Num, h::UInt) = _symhash(unwrap(x), h)

# ── zero / one ───────────────────────────────────────────────────────────────
Base.zero(::Num) = Num(0)
Base.zero(::Type{Num}) = Num(0)
Base.one(::Num) = Num(1)
Base.one(::Type{Num}) = Num(1)
Base.iszero(x::Num) = _iszeroc(unwrap(x))
Base.isone(x::Num) = _isonec(unwrap(x))
