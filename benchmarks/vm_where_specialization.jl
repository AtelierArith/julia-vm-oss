# Driver for vm_where_specialization_benchmark (Issue #6868).
#
# Calls a `where`-parametric method `f(x::T) where T<:Real` in a hot nested loop
# with a concrete Float64 argument. Before #6868 the direct-call path ran the
# method's unspecialized generic body (every param bound to `Any`, inner
# operators dynamically dispatched); the fix specializes the body for the
# concrete runtime argument types on the direct-call path. The accumulated sum
# is printed so the benchmark can validate correctness.
function sinc_w(x::T) where T<:Real
    if x == 0
        return 1.0
    end
    px = pi * x
    return sin(px) / px
end

function run_where()
    s = 0.0
    for k in 1:200
        for i in 1:100
            s += sinc_w(0.001 * i)
        end
    end
    return s
end

println(run_where())
