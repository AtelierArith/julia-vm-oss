using Test

@testset "Iterators.filter tuple stateful iterate (Issue #8370)" begin
    filt = Iterators.filter(x -> true, (1, 2, 3))
    first_step = iterate(filt)
    second_step = iterate(filt, first_step[2])

    @test first_step == (1, 2)
    @test second_step == (2, 3)
end

true
