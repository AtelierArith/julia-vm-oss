using Test

@testset "multiline tuple literals after assignment and in (#4654)" begin
    xs = (
        ("x", 1),
        ("y", 2)
    )

    @test length(xs) == 2
    @test xs[1][1] == "x"
    @test xs[1][2] == 1
    @test xs[2][1] == "y"
    @test xs[2][2] == 2

    names = String[]
    values = Int64[]
    for (name, value) in (
        ("x", 1),
        ("y", 2)
    )
        push!(names, name)
        push!(values, value)
    end

    @test length(names) == 2
    @test names[1] == "x"
    @test names[2] == "y"
    @test values[1] == 1
    @test values[2] == 2
end

true
