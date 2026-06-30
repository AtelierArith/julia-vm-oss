using Test
using Interact, Plots

# Issue #7275: `@manipulate for var = choices … end` (Interact MVP) evaluates the
# body once per discrete choice and collects the per-choice `Plot`s plus their
# labels into a `Manipulate`. The Rust artifact pipeline renders this as a static
# Plotly figure with a dropdown (asserted in plot_artifact_mime_tests.rs); here we
# pin the Julia-level behavior: the right number of plots, the labels, and that each
# captured value is a `Plot`.
@testset "Interact: @manipulate collects one Plot per discrete choice" begin
    datasets = Dict(:some => [1.0, 4.0, 9.0, 16.0], :other => [2.0, 3.0, 5.0, 7.0])
    m = @manipulate for dataset = [:some, :other]
        scatter(datasets[dataset])
    end

    @test isa(m, Manipulate)
    @test length(m.plots) == 2
    @test m.labels == ["some", "other"]
    @test isa(m.plots[1], Plot)
    @test isa(m.plots[2], Plot)
end

@testset "Interact: @manipulate over a range labels by value" begin
    m = @manipulate for n = 1:3
        plot([1.0, 2.0, 3.0], [n, 2.0 * n, 3.0 * n])
    end

    @test isa(m, Manipulate)
    @test length(m.plots) == 3
    @test m.labels == ["1", "2", "3"]
end

# Issue #7338: control kind follows upstream `widget()` dispatch — an `AbstractRange`
# choice is a continuous slider, everything else stays a discrete dropdown.
@testset "Interact: @manipulate control kind (range → slider, array → dropdown)" begin
    mr = @manipulate for n = 1:3
        plot([1.0, 2.0], [n, 2.0 * n])
    end
    @test mr.control == :slider

    ma = @manipulate for c = [:a, :b]
        plot([1.0, 2.0], [1.0, 2.0])
    end
    @test ma.control == :dropdown

    # `manipulate_control` maps the choices value to its control kind directly.
    @test manipulate_control(1:5) == :slider
    @test manipulate_control(0.0:0.1:1.0) == :slider
    @test manipulate_control([1, 2, 3]) == :dropdown
    @test manipulate_control([:a, :b]) == :dropdown
end

true
