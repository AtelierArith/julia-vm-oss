# Issue #6601: a FunctionRef Assign-RHS slot (`f = sin`) is typed without the
# legacy pre-scan path; the function value bound to the slot stays callable
# through that slot. The seam reproduces the legacy `ValueType::Function`
# result for FunctionRef via a scoped shim (the shared engine would widen to
# Any), so `f = sin; g = cos; f(0.0) + g(0.0)` evaluates to sin(0)+cos(0) == 1.0.
function eval_two()
    f = sin
    g = cos
    return f(0.0) + g(0.0)
end

@assert eval_two() == 1.0
@assert typeof(eval_two()) === Float64

# A user-defined function bound to a FunctionRef slot is likewise callable.
square(x) = x * x

function apply_square()
    h = square
    return h(3.0)
end

@assert apply_square() == 9.0
@assert typeof(apply_square()) === Float64

println(eval_two())

true
