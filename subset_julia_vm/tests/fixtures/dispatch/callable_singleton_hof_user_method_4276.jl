using Test

import Base: broadcast, map

map(::typeof(identity), xs::Vector{Int64}) = [:user_map_4276]
broadcast(::typeof(+), xs::Vector{Int64}, ys::Vector{Int64}) = [:user_broadcast_4276]

map_callable_any_4276(f, xs::Any) = map(f, xs)
broadcast_callable_any_4276(f, xs::Any, ys::Any) = broadcast(f, xs, ys)

@testset "callable singleton HOF dispatch before fallback (Issue #4276)" begin
    @test map(identity, [1, 2, 3]) == [:user_map_4276]
    @test map_callable_any_4276(identity, [1, 2, 3]) == [:user_map_4276]

    @test broadcast(+, [1, 2], [10, 20]) == [:user_broadcast_4276]
    @test broadcast_callable_any_4276(+, [1, 2], [10, 20]) == [:user_broadcast_4276]

    @test map(x -> x + 1, [1, 2]) == [2, 3]
end

true
