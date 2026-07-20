using Test

hits = Int[]

@testset "newline multi-iterator @testset for" for T in [Int8],
    U in [Int8, Int16]
    push!(hits, sizeof(T) + sizeof(U))
    @test T(1) == U(1)
end

@test hits == [2, 3]

true
