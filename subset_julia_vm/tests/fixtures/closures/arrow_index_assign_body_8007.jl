# An arrow lambda whose single-expression body is an index/field assignment must
# lower to a closure that performs the mutation, matching upstream Julia
# (Issue #8007). Previously `x -> (x[2] = v)` failed with "missing lambda body"
# (named form) or "assignment target must be identifier" (anonymous form), because
# the lambda-body lowering misrouted a parenthesized body into the parameter list
# and the expression-context assignment path rejected non-identifier targets.

struct ArrowWrap8007
    u::Vector{Float64}
end

mutable struct ArrowMut8007
    z::Float64
end

function closures_arrow_index_assign_body_8007()
    # Named arrow with index-assignment body mutates the array.
    v = [1.0, 2.0]
    g = x -> (x[2] = 99.0)
    g(v)
    ok1 = v == [1.0, 99.0]

    # Named arrow with field-index-assignment body (x.u[2] = v).
    s = ArrowWrap8007([1.0, 2.0])
    h = x -> (x.u[2] = 5.0)
    h(s)
    ok2 = s.u == [1.0, 5.0]

    # Named arrow mutating a mutable struct field.
    p = ArrowMut8007(1.0)
    f = q -> (q.z = 42.0)
    f(p)
    ok3 = p.z == 42.0

    # Anonymous arrow with index-assignment body passed to a higher-order function.
    w = [10.0, 20.0]
    map(x -> (x[1] = -x[1]), [w])
    ok4 = w == [-10.0, 20.0]

    # A plain parenthesized (non-assignment) body must still lower correctly.
    inc = x -> (x + 1)
    ok5 = inc(5) == 6

    # Index assignment used as a value expression yields the assigned RHS value.
    a = [0.0, 0.0]
    y = (a[1] = 3.0)
    ok6 = (a == [3.0, 0.0]) && (y == 3.0)

    ok1 && ok2 && ok3 && ok4 && ok5 && ok6
end

closures_arrow_index_assign_body_8007()
