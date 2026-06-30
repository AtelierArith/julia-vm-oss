# Distributions: integer-argument Normal constructor (Issue #7284)
#
# `Normal(2, 3)` promotes its integer arguments to `Float64` inside the outer
# constructor (`m, s = promote(float(μ), float(σ)); Normal{typeof(m)}(m, s)`),
# so the runtime value is `Normal{Float64}`. The compiler used to infer the
# constructor's return type as `Normal{Int64}` from the integer literal args,
# typing the field load as `Int64`; the runtime `Float64` field then failed with
# "Type error: expected I64, got Float64" on an inline field/method access.

using Distributions

ok = true

# Constructor return type promotes integer args to Float64.
ok = ok && (typeof(Normal(2, 3)) == Normal{Float64})

# Inline method access on the integer-arg distribution (the failing form).
ok = ok && (mean(Normal(2, 3)) == 2.0)
ok = ok && (var(Normal(2, 3)) == 9.0)
ok = ok && (std(Normal(2, 3)) == 3.0)

# Inline direct field access on the integer-arg distribution.
ok = ok && (Normal(2, 3).μ == 2.0)
ok = ok && (Normal(2, 3).σ == 3.0)

# The float-literal form was always fine; keep it as a regression.
ok = ok && (mean(Normal(2.0, 3.0)) == 2.0)

ok
