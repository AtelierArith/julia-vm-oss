# Issue #6601: UnaryOp Assign-RHS slots typed through the shared engine.
function neg_complex()
    c = 1.0 + 2.0im
    d = -c
    return d
end
function logical_not(b)
    x = !b
    return x
end
@assert neg_complex() == -1.0 - 2.0im
@assert typeof(neg_complex()) === ComplexF64
@assert logical_not(true) === false
@assert typeof(logical_not(true)) === Bool
true
