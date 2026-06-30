using Test

import Base: map

map(f::Function, a::Vector{Int64}, b::Vector{Int64}) = [4019, length(a), length(b)]

map_binary_any_4019(a::Any, b::Any) = map((x, y) -> x + y, a, b)

@testset "binary map Any runtime dispatch (Issue #4019)" begin
    @test map((x, y) -> x + y, [1, 2], [3, 4]) == [4019, 2, 2]
    @test map_binary_any_4019([1, 2], [3, 4]) == [4019, 2, 2]
end

true
