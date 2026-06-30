using Test

@testset "typed vector equality dispatch (#4639)" begin
    @test Int16[1, 2] == Int16[1, 2]
    @test !(Int16[1, 2] == Int16[1, 3])
    @test Int16[1, 2] != Int16[1, 3]

    xs = Int16[1, 2]
    ys = Int16[1, 2]
    zs = Int16[1, 3]
    @test xs == ys
    @test xs != zs
    @test typeof(xs == ys) === Bool
end

true
