using Test
using Plots

# Issue #7302: a type exported by a bundled package is reachable unqualified
# (`Plot`, via `export`) but the *qualified* form `Plots.Plot` previously failed
# to resolve — the qualified access was mis-routed as a module *function* lookup
# and errored with "Module Plots has no function named Plot". Upstream Julia
# always allows `Module.Type`. (The sjulia Plots subset's `Plot` is a plain
# struct; the qualified type resolution is the upstream-portable part here.)

@testset "qualified Plots.Plot type access (Issue #7302)" begin
    p = plot([1.0, 2.0, 3.0])
    @test isa(p, Plot)         # unqualified (export)
    @test isa(p, Plots.Plot)   # qualified
    @test Plots.Plot === Plot  # qualified type value is the same type
    @test p isa Plots.Plot     # `x isa Module.T` infix form
end

true
