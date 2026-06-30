using Test
using Interact, Plots

# Issue #7344: `@manipulate for a = …, b = … end` (multiple simultaneous controls).
# Upstream gives each variable its own reactive control and re-evaluates on any
# change. With no reactive runtime, sjulia approximates that as ONE static dropdown
# over the *cartesian product* of all choices — every combination is selectable and
# labelled `a=<va>, b=<vb>, …`. The body is evaluated once per combination via nested
# loops (inner-most variable varies fastest), so this is an intentional divergence
# from upstream's N independent controls (documented in UNIMPLEMENTED.md).

@testset "Interact: @manipulate over two controls builds the cartesian product" begin
    m = @manipulate for a = 1:2, b = 1:3
        plot([1.0, 2.0], [a * 1.0, b * 1.0])
    end

    @test isa(m, Manipulate)
    # 2 × 3 = 6 combinations, one Plot each.
    @test length(m.plots) == 6
    @test all(p -> isa(p, Plot), m.plots)
    # Combined labels, inner-most variable (`b`) varying fastest.
    @test m.labels == [
        "a=1, b=1", "a=1, b=2", "a=1, b=3",
        "a=2, b=1", "a=2, b=2", "a=2, b=3",
    ]
    # The cartesian-product control renders as a single combined dropdown.
    @test m.control == :dropdown
end

@testset "Interact: @manipulate over three controls" begin
    m = @manipulate for i = 1:2, j = 1:2, k = 1:2
        plot([1.0, 2.0], [Float64(i), Float64(j + k)])
    end
    @test length(m.plots) == 8                # 2 × 2 × 2
    @test m.labels[1] == "i=1, j=1, k=1"
    @test m.labels[end] == "i=2, j=2, k=2"
    @test m.control == :dropdown
end

@testset "Interact: multi-control non-plot body still errors clearly" begin
    @test_throws ErrorException begin
        @manipulate for a = 1:2, b = 1:2
            a + b
        end
    end
end

true
