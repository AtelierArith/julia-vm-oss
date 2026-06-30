# Pretty-printing for the Symbolics subset (Issue #6572).
#
# Infix rendering with operator-precedence parenthesization, modelled on
# `base/complex.jl`'s single 2-arg `Base.show(io::IO, ::Complex)` method: the VM
# routes `string`/`print`/`repr`/REPL display through the registered user `show`
# method (keyed on the struct's base name, so `Num`/`Sym`/`Term` don't collide).
#
# Display is intentionally "loose" vs. upstream Symbolics (Issue #6572): no
# canonical term ordering, `2*x` rather than `2x`. Equivalence is checked with
# `substitute`/`isequal`, not exact display strings.

# Operator precedence (higher binds tighter). Atoms (Sym, Number, function call)
# sit above every infix operator so they are never parenthesized.
_opprec(op::Symbol) =
    op === :+ ? 1 :
    op === :- ? 1 :
    op === :* ? 2 :
    op === :/ ? 2 :
    op === :^ ? 3 : 10

_isinfix(op::Symbol) = op === :+ || op === :- || op === :* || op === :/ || op === :^

_infixstr(op::Symbol) =
    op === :+ ? " + " :
    op === :- ? " - " :
    op === :* ? "*" :
    op === :/ ? "/" :
    op === :^ ? "^" : string(op)

# Precedence of a node, used to decide whether it needs parentheses inside a
# parent context that requires at least `minprec`. Inspect `Term`s via the
# `operation`/`arguments` accessors — direct `x.args` on a dynamically-typed
# value mis-routes to the builtin `Expr.args` accessor (Issue #7162's field
# collision).
_nodeprec(x) =
    (x isa Term && _isinfix(operation(x)) && length(arguments(x)) == 2) ? _opprec(operation(x)) :
    (x isa Term && operation(x) === :- && length(arguments(x)) == 1) ? 2 : 10

# Decide whether an addend renders with a leading minus, and return its positive
# magnitude (Issue #7894). A `(-1)*t` product prints as `-t`, a `(-c)*t` product
# (c > 0) as `-(c*t)`, and a negative number as `-n`, so a sum renders `a - b`
# instead of `a + (-1)*b` — matching upstream Symbolics' negative-coefficient
# display. Everything else is unchanged (`false`, the term itself).
function _addend_sign(t)::Any
    if t isa Number
        return (t < 0, t < 0 ? -t : t)
    elseif t isa Term && operation(t) === :* && length(arguments(t)) == 2 &&
           arguments(t)[1] isa Number && arguments(t)[1] < 0
        c = arguments(t)[1]
        rest = arguments(t)[2]
        return (true, c == -1 ? rest : Term(:*, Any[-c, rest]))
    else
        return (false, t)
    end
end

# Print `x`, wrapping in parentheses when its precedence is below `minprec`.
function _printnode(io::IO, x, minprec::Int)::Nothing
    needs = _nodeprec(x) < minprec
    needs && print(io, "(")
    _printbare(io, x)
    needs && print(io, ")")
    nothing
end

function _printbare(io::IO, x)::Nothing
    if x isa Sym
        print(io, x.name)
    elseif x isa Term
        op = operation(x)
        args = arguments(x)
        if _isinfix(op) && length(args) == 2
            p = _opprec(op)
            if op === :+
                # Render the right addend sign-aware: `a + (-1)*b` prints `a - b`.
                _printnode(io, args[1], p)
                neg, mag = _addend_sign(args[2])
                print(io, neg ? " - " : " + ")
                _printnode(io, mag, p + 1)
            elseif op === :* && args[1] isa Number && args[1] < 0
                # A negative-coefficient product prints with a leading minus.
                _, mag = _addend_sign(x)
                print(io, "-")
                _printnode(io, mag, p)
            elseif op === :^
                # right-associative: the left operand needs parens at equal prec
                _printnode(io, args[1], p + 1)
                print(io, _infixstr(op))
                _printnode(io, args[2], p)
            else
                # left-associative: the right operand needs parens at equal prec
                _printnode(io, args[1], p)
                print(io, _infixstr(op))
                _printnode(io, args[2], p + 1)
            end
        elseif op === :- && length(args) == 1
            print(io, "-")
            _printnode(io, args[1], 3)   # parenthesize sums under unary minus
        else
            # function application: op(a, b, ...)
            print(io, op, "(")
            for i in eachindex(args)
                i > 1 && print(io, ", ")
                _printnode(io, args[i], 0)
            end
            print(io, ")")
        end
    else
        # Number or anything else
        print(io, x)
    end
    nothing
end

# `Num` is transparent: print the wrapped value (mirrors upstream `Num` show).
Base.show(io::IO, x::Num) = _printnode(io, unwrap(x), 0)
Base.show(io::IO, x::Sym) = _printnode(io, x, 0)
Base.show(io::IO, x::Term) = _printnode(io, x, 0)
