using Test
using Plots

@testset "Plots: title! (Issue #7850)" begin
    p = plot([1, 2, 3], [1.0, 2.0, 3.0])
    @test p.title == ""

    p2 = title!(p, "My Title")
    @test p2.title == "My Title"
    @test p.title == ""  # original is unchanged

    p3 = plot([1, 2], [4.0, 5.0]; title="Initial")
    @test p3.title == "Initial"
    p4 = title!(p3, "Changed")
    @test p4.title == "Changed"

    # No-argument form applies to current()
    p5 = plot([1, 2], [1.0, 2.0])
    p6 = title!("Bare Title")
    @test p6.title == "Bare Title"
    @test current().title == "Bare Title"
end

true
