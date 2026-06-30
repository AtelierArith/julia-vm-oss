# `substitute` for the Symbolics subset (Issue #6572).
#
# Replace symbolic variables (`Sym`) in an expression with values, rebuilding the
# tree through the shallow normalization constructors (`_rebuild`) so that fully
# numeric substitutions fold to a number. A reduced port of
# `extern/SymbolicUtils.jl/src/substitute.jl` (whose `Substituter` also supports
# fixpoint iteration, fold control and metadata — out of scope for the core set).

"""
    substitute(expr, dict::AbstractDict)
    substitute(expr, pair::Pair)

Substitute every occurrence of each key (a symbolic variable) in `expr` with the
corresponding value, returning a `Num`. Numeric substitutions fold:

```julia
@variables x y
substitute(x^2 + 1, x => 3)          # 10
substitute(x*y, Dict(x => 2, y => 5)) # 10
substitute(x + y, x => 3)            # 3 + y   (partial; stays symbolic)
```
"""
substitute(expr, dict::AbstractDict)::Num = Num(_subst(unwrap(expr), dict))
substitute(expr, pair::Pair)::Num = Num(_subst(unwrap(expr), Dict(pair)))

# Walk a bare node (Number / Sym / Term), replacing matched `Sym`s and rebuilding
# `Term`s through `_rebuild` so numeric results collapse.
function _subst(node, dict)::Any
    if node isa Sym
        for (k, v) in dict
            ku = unwrap(k)
            if ku isa Sym && ku.name === node.name
                return unwrap(v)
            end
        end
        return node
    elseif node isa Term
        newargs = Any[]
        for a in arguments(node)
            push!(newargs, _subst(a, dict))
        end
        return _rebuild(operation(node), newargs)
    else
        return node
    end
end
