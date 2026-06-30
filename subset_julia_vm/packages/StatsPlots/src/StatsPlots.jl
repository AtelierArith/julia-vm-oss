module StatsPlots

# A pure-Julia subset of StatsPlots.jl for SubsetJuliaVM (Issue #7262).
#
# Upstream StatsPlots wires Distributions into Plots through a `@recipe` for
# `AbstractVector{<:Distribution}` / `Distribution`, sampling the pdf (continuous)
# or pmf (discrete) over `quantile(d, 0.0001) … quantile(d, 0.9999)` and emitting a
# line / sticks series (extern reference: StatsPlots.jl/src/distributions.jl).
#
# This port reproduces only the univariate-distribution plotting recipe: it adds
# `plot` / `plot!` methods that turn a `Distribution` into the same artifact the
# existing Plots backend already renders, so
#
#     using Distributions, StatsPlots
#     plot(Normal(0, 1))
#
# draws the bell-curve pdf. The remaining StatsPlots features (`@df`, `corrplot`,
# `marginalhist`, `boxplot`, `violin`, `density`, sample-based `histogram(rand(d,
# n))`) are out of scope for this issue.

using Plots
using Distributions

# Extend (not shadow) the Plots generics with distribution methods.
import Plots: plot, plot!

include("distributions.jl")

# Re-export the common Plots drawing API, mirroring upstream StatsPlots, so that a
# single `using StatsPlots` is enough to call `plot` / `scatter` / `bar` / … on
# both ordinary data and distributions.
export plot, plot!, scatter, scatter!, bar, bar!, histogram, histogram!

end # module StatsPlots
