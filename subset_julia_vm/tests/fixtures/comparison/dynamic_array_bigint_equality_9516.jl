using Test

erase_9516(x) = x

@testset "dynamic array == compares Any BigInt elements by numeric value (Issues #9516/#10270/#10290)" begin
    v = Vector{Any}(undef, 3)
    v[1] = big(1)
    v[2] = big(2)
    v[3] = big(3)

    @test v == [1, 2, 3]
    @test erase_9516(v) == [1, 2, 3]
    @test isequal(erase_9516(v), [1, 2, 3])

    @test erase_9516(Any[big(1), big(2)]) == [1.0, 2.0]
    @test erase_9516(Any[big(0)]) == [-0.0]
    @test [-0.0] == erase_9516(Any[big(0)])
    @test !(erase_9516(Any[big(0)]) != [-0.0])
    @test !isequal(erase_9516(Any[big(0)]), [-0.0])

    @test !(erase_9516(Any[big(9007199254740993)]) == [9007199254740992.0])
    @test erase_9516(Any[big(9007199254740992)]) == [9007199254740992.0]
end

true
