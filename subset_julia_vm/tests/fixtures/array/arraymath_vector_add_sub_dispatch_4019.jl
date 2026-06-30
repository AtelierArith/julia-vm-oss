using Test

@testset "Vector arraymath dispatch-first (Issue #4019)" begin
    @test [1.0, 2.0, 3.0] + [10.0, 20.0, 30.0] == [11.0, 22.0, 33.0]
    @test [10, 20, 30] - [1, 2, 3] == [9, 18, 27]

    Base.:+(a::Vector{Int64}, b::Vector{Int64}) = [4019, length(a), length(b)]
    @test [1, 2] + [3, 4] == [4019, 2, 2]
end

true
