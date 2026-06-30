module NaNMath

# Minimal NaNMath compatibility stub for the Optim.jl MVP (Issue #7478).
#
# Upstream NaNMath provides math functions that return `NaN` instead of
# throwing a `DomainError` for out-of-domain inputs (e.g. `sqrt(-1.0)`), which
# some Optim solvers use to keep optimizing through transiently infeasible
# trial points.  The MVP solvers (GoldenSection, Brent, NelderMead,
# GradientDescent) never route through NaNMath, so this stub only exposes the
# handful of names so that `using NaNMath` resolves.  The functions delegate to
# Base but substitute `NaN` for negative-domain inputs.

export sqrt, log, log2, log10, pow

sqrt(x) = x < zero(x) ? oftype(float(x), NaN) : Base.sqrt(float(x))
log(x) = x < zero(x) ? oftype(float(x), NaN) : Base.log(float(x))
log2(x) = x < zero(x) ? oftype(float(x), NaN) : Base.log2(float(x))
log10(x) = x < zero(x) ? oftype(float(x), NaN) : Base.log10(float(x))
pow(x, y) = Base.:^(x, y)

end # module NaNMath
