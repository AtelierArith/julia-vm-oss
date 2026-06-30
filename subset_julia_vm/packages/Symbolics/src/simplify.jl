# `simplify` and `expand` for the Symbolics subset (Issue #6572).
#
# A reduced port of the rule-based simplifier in
# `extern/SymbolicUtils.jl/src/simplify_rules.jl` and the polynomial `expand`.
# Upstream runs a fixpoint of `@rule`/`@acrule` rewrites; the subset does a single
# bottom-up pass that:
#   * flattens nested `+`/`*` chains,
#   * folds numeric constants,
#   * combines like terms in sums (`x + x → 2x`, `2x + 3x → 5x`) and like factors
#     in products (`x*x → x^2`, `x^2*x → x^3`),
#   * canonically orders commutative operands so reordered terms combine
#     (`x*y + y*x → 2*x*y`, `(x+y)^2 → x^2 + 2*x*y + y^2`).
#
# `expand` distributes products over sums (and small integer powers) and then
# runs `simplify` to collect the resulting polynomial.
#
# Because operands are canonically sorted, the *order* of the output may differ
# from a naive construction; verify equivalence by `substitute`-evaluation or
# `isequal` against the canonical form, not by string comparison.

# ── Canonical ordering key ───────────────────────────────────────────────────
# A structural string used only to impose a deterministic, commutative-friendly
# order on `+` addends and `*` factors.
function _canonkey(x)::String
    if x isa Sym
        "s:" * string(x.name)
    elseif x isa Number
        "n:" * string(x)
    elseif x isa Term
        parts = String[]
        for a in arguments(x)
            push!(parts, _canonkey(a))
        end
        "t:" * string(operation(x)) * "(" * join(parts, ",") * ")"
    else
        string(x)
    end
end

# Sort `v` by `_canonkey`. Computed by building the key vector with a direct loop
# and `sortperm`, rather than `sort(v; by = _canonkey)` — passing the module
# helper `_canonkey` to a Base HOF does not resolve in this VM's module scope
# (same reason as `_findbase`).
function _sortbykey(v)::Any
    keys = String[]
    for x in v
        push!(keys, _canonkey(x))
    end
    return v[sortperm(keys)]
end

# ── Flatten / fold helpers ───────────────────────────────────────────────────
# Collect the operands of a flattened `op` chain (e.g. `a + (b + c)` → [a,b,c]).
function _flatten(op::Symbol, node)::Vector{Any}
    if node isa Term && operation(node) === op
        out = Any[]
        for a in arguments(node)
            append!(out, _flatten(op, a))
        end
        out
    else
        Any[node]
    end
end

# Left-fold the normalization constructors over a list. Written as explicit
# loops rather than `reduce(_mkmul, …)` for the same module-scope reason as
# `_findbase` (a module helper passed to a Base HOF does not resolve).
function _foldmul(factors)::Any
    isempty(factors) && return 1
    acc = factors[1]
    for i in 2:length(factors)
        acc = _mkmul(acc, factors[i])
    end
    acc
end
function _foldadd(terms)::Any
    isempty(terms) && return 0
    acc = terms[1]
    for i in 2:length(terms)
        acc = _mkadd(acc, terms[i])
    end
    acc
end

# Index of the first structurally-equal base, or 0 if absent. Written as an
# explicit loop rather than `findfirst(x -> _structeq(x, bases))`: a module
# helper referenced from a lambda passed to a Base HOF does not resolve in this
# VM's module scope ("function '_structeq' is not imported", Issue #7180),
# whereas a direct call does.
function _findbase(bases, b)::Int
    for j in eachindex(bases)
        _structeq(bases[j], b) && return j
    end
    return 0
end

# Split a sum addend into (numeric coefficient, base product) for like-term
# grouping: `2*x*y → (2, x*y)`, `x → (1, x)`, a bare number `n → (n, 1)`.
function _split_coeff(term)::Any
    if term isa Number
        return (term, 1)
    end
    if term isa Term && operation(term) === :*
        coeff = 1
        rest = Any[]
        for f in _flatten(:*, term)
            if f isa Number
                coeff = coeff * f
            else
                push!(rest, f)
            end
        end
        return (coeff, isempty(rest) ? 1 : _foldmul(rest))
    end
    return (1, term)
end

# Split a product factor into (base, exponent) for like-factor grouping:
# `x^3 → (x, 3)`, `x → (x, 1)`.
function _split_pow(f)::Any
    if f isa Term && operation(f) === :^ && length(arguments(f)) == 2 &&
       arguments(f)[2] isa Number
        return (arguments(f)[1], arguments(f)[2])
    end
    return (f, 1)
end

# ── Sum-addend ordering (Issue #7894) ────────────────────────────────────────
# Order a sum's addends to match upstream Symbolics' display for the monomial
# sums that `det`/`inv` produce: ascending total degree, then (within a degree)
# the addend with the larger exponent on the lexicographically-earlier variable
# first (so `x^2` precedes `x*y` precedes `y^2`, and a constant leads). Full
# SymbolicUtils polynomial canonicalization (and the `2x` coefficient spelling)
# is out of scope; equivalence is still checked structurally, not by string.

# Monomial exponent profile of an addend (numeric coefficient stripped): parallel
# vectors of canonical variable keys and their integer exponents.
function _addend_exponents(term)::Any
    vars = String[]
    exps = Int[]
    _, base = _split_coeff(term)
    (base === 1 || base isa Number) && return (vars, exps)
    for f in _flatten(:*, base)
        b, e = _split_pow(f)
        push!(vars, _canonkey(b))
        push!(exps, (e isa Number) ? Int(e) : 1)
    end
    return (vars, exps)
end

function _sort_addends(addends)::Any
    length(addends) <= 1 && return addends
    allvars = String[]
    varlists = Any[]
    explists = Any[]
    for t in addends
        vs, es = _addend_exponents(t)
        push!(varlists, vs)
        push!(explists, es)
        for v in vs
            (v in allvars) || push!(allvars, v)
        end
    end
    allvars = sort(allvars)
    keys = String[]
    for k in eachindex(varlists)
        vs = varlists[k]
        es = explists[k]
        deg = 0
        for e in es
            deg += e
        end
        s = lpad(string(deg), 6, '0')
        for v in allvars
            e = 0
            for j in eachindex(vs)
                vs[j] == v && (e = es[j])
            end
            # Descending exponent within a degree: encode as a complement so the
            # larger exponent yields the smaller (earlier-sorting) key.
            s = s * "_" * lpad(string(100000 - e), 6, '0')
        end
        push!(keys, s)
    end
    return addends[sortperm(keys)]
end

# ── simplify ─────────────────────────────────────────────────────────────────
# `sargs` are already-simplified operands.
function _simplify_add(sargs)::Any
    flat = Any[]
    for a in sargs
        append!(flat, _flatten(:+, a))
    end
    numsum = 0
    bases = Any[]
    coeffs = Any[]
    for t in flat
        c, b = _split_coeff(t)
        if b === 1 || b isa Number
            numsum += c * (b isa Number ? b : 1)
            continue
        end
        idx = _findbase(bases, b)
        if idx == 0
            push!(bases, b)
            push!(coeffs, c)
        else
            coeffs[idx] += c
        end
    end
    keep = Any[]
    for i in eachindex(bases)
        _iszeroc(coeffs[i]) && continue
        push!(keep, _mkmul(coeffs[i], bases[i]))
    end
    # The numeric constant is a degree-0 addend, sorted with the rest (it leads,
    # e.g. `-1 + x^2`), not appended last.
    numsum != 0 && push!(keep, numsum)
    keep = _sort_addends(keep)
    return _foldadd(keep)
end

function _simplify_mul(sargs)::Any
    flat = Any[]
    for a in sargs
        append!(flat, _flatten(:*, a))
    end
    numprod = 1
    bases = Any[]
    exps = Any[]
    for f in flat
        if f isa Number
            numprod = numprod * f
            continue
        end
        b, e = _split_pow(f)
        idx = _findbase(bases, b)
        if idx == 0
            push!(bases, b)
            push!(exps, e)
        else
            exps[idx] += e
        end
    end
    _iszeroc(numprod) && return 0
    factors = Any[]
    for i in eachindex(bases)
        push!(factors, _mkpow(bases[i], exps[i]))
    end
    factors = _sortbykey(factors)
    numprod != 1 && pushfirst!(factors, numprod)
    return _foldmul(factors)
end

# Bottom-up simplification of a bare node. Subtraction is rewritten as
# `a + (-1)*b` (and unary `-a` as `(-1)*a`) so it flows through the additive
# like-term combination.
function _simplify(node)::Any
    node isa Term || return node
    op = operation(node)
    sargs = Any[]
    for a in arguments(node)
        push!(sargs, _simplify(a))
    end
    if op === :+
        _simplify_add(sargs)
    elseif op === :- && length(sargs) == 2
        _simplify_add(Any[sargs[1], _mkmul(-1, sargs[2])])
    elseif op === :- && length(sargs) == 1
        _simplify_mul(Any[-1, sargs[1]])
    elseif op === :*
        _simplify_mul(sargs)
    else
        _rebuild(op, sargs)
    end
end

"""
    simplify(expr)

Simplify a symbolic expression: fold constants, combine like terms and like
factors, and canonically order commutative operands. Returns a `Num`.

```julia
@variables x y
simplify(x + x)        # 2x
simplify(2x + 3x)      # 5x
simplify(x*x)          # x^2
simplify(x + y + x)    # 2x + y   (up to canonical ordering)
```
"""
simplify(expr)::Num = Num(_simplify(unwrap(expr)))

# ── expand ───────────────────────────────────────────────────────────────────
_as_addends(f)::Vector{Any} = (f isa Term && operation(f) === :+) ? _flatten(:+, f) : Any[f]

# Distribute a product of factors (each possibly a sum) into a sum of products.
function _expand_product(factors)::Any
    terms = Any[Any[]]   # start from the empty product (= 1)
    for f in factors
        adds = _as_addends(f)
        newterms = Any[]
        for t in terms
            for a in adds
                push!(newterms, vcat(t, Any[a]))
            end
        end
        terms = newterms
    end
    summed = Any[]
    for t in terms
        push!(summed, _foldmul(t))
    end
    return _foldadd(summed)
end

# Bound on integer powers we expand by repeated multiplication; larger powers are
# left as a `^` Term to avoid blow-up.
const _EXPAND_POW_MAX = 8

function _expand(node)::Any
    node isa Term || return node
    op = operation(node)
    eargs = Any[]
    for a in arguments(node)
        push!(eargs, _expand(a))
    end
    if op === :*
        _expand_product(eargs)
    elseif op === :^ && length(eargs) == 2 && eargs[2] isa Integer &&
           eargs[2] >= 2 && eargs[2] <= _EXPAND_POW_MAX
        r = eargs[1]
        for _ in 2:eargs[2]
            r = _expand_product(Any[r, eargs[1]])
        end
        r
    else
        _rebuild(op, eargs)
    end
end

"""
    expand(expr)

Expand products over sums (and small integer powers), then `simplify` the
result. Returns a `Num`.

```julia
@variables x y
expand(x*(x + 1))       # x^2 + x
expand((x + 1)*(x + 2)) # x^2 + 3x + 2
expand((x + y)^2)       # x^2 + 2*x*y + y^2
```
"""
expand(expr)::Num = Num(_simplify(_expand(unwrap(expr))))
