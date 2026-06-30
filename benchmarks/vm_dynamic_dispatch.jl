# VM-only benchmark for the per-dynamic-call frame-setup cost (Issue #6853).
#
# `norm2` and `sinc_approx` take untyped parameters, so every call goes through
# the runtime dynamic-dispatch path (`CallDynamic` / `start_function_call`).
# Before #6853 that path cloned the *entire* selected `FunctionInfo`
# (`name`/`params`/`param_julia_types`/`slot_names`/... — many `Vec`/`String`)
# on every call just to release the `self.functions[idx]` borrow before taking
# `&mut self` for frame setup. The double loop runs ~2 dynamic calls per point
# over 10000 points, so the clone cascade showed up as steady-state overhead.
# With `Vm.functions: Vec<Rc<FunctionInfo>>` the per-call clone is an O(1)
# refcount bump.

function norm2(v)
    s = 0.0
    for x in v
        s += x * x
    end
    return sqrt(s)
end

function sinc_approx(x)
    if x == 0.0
        return 1.0
    end
    return sin(x) / x
end

function kernel(n)
    acc = 0.0
    for i in 1:n
        x = Float64(i % 17) - 8.0
        y = Float64(i % 13) - 6.0
        acc += sinc_approx(norm2([x, y]))
    end
    return acc
end

println(kernel(10000))
