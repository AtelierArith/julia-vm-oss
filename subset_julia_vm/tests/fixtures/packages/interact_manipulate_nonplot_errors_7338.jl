using Test
using Interact, Plots

# Issue #7338: a non-plot `@manipulate` body used to silently build a `Manipulate`
# whose `plots` held raw values (e.g. `Any[1, 4, 9]`), evaluate to exit 0, and
# render nothing. Upstream Interact renders non-plot bodies (numbers/strings/HTML)
# through a reactive widget runtime, which is out of scope for this static-Plotly
# MVP (see Issue #7275). The MVP now errors clearly instead.
#
# NOTE: this intentionally diverges from upstream `julia` (which shows the value),
# so `scripts/fixture_julia_parity.sh` is not expected to match here — the clear
# error is the documented sjulia MVP behavior. (The validation lives in the
# `@manipulate` expansion rather than `Manipulate`'s inner constructor because
# sjulia ignores inner constructor bodies — Issue #7345.)
@testset "Interact: @manipulate with a non-plot body errors clearly" begin
    @test_throws ErrorException begin
        @manipulate for x = 1:3
            x^2
        end
    end

    @test_throws ErrorException begin
        @manipulate for s = ["a", "b"]
            s
        end
    end
end

# A plot-producing body must still build a valid `Manipulate` (regression guard).
@testset "Interact: @manipulate with a plot body still works" begin
    m = @manipulate for n = 1:3
        plot([1.0, 2.0, 3.0], [n, 2.0 * n, 3.0 * n])
    end
    @test isa(m, Manipulate)
    @test length(m.plots) == 3
    @test all(p -> isa(p, Plot), m.plots)
end

true
