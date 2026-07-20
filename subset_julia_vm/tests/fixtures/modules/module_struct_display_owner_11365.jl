# Module-owned struct display follows upstream visibility: a type reachable
# unqualified from Main (using-imported export) prints bare; a module-private
# or never-imported type prints the full path from the top (Main.M.B), for
# both instance display and typeof (Issue #11365).

using Test

module Geo11365
export Point11365
struct Point11365{T}
    x::T
    y::T
end
struct Hidden11365
    v::Int
end
end

module Plain11365
struct B end
end

using .Geo11365

@testset "module struct display owner qualification (Issue #11365)" begin
    @test string(Point11365(1, 2)) == "Point11365{Int64}(1, 2)"
    @test string(Geo11365.Point11365(3, 4)) == "Point11365{Int64}(3, 4)"
    @test string(typeof(Point11365(1, 2))) == "Point11365{Int64}"
    @test string(Geo11365.Hidden11365(7)) == "Main.Geo11365.Hidden11365(7)"
    @test string(typeof(Geo11365.Hidden11365(7))) == "Main.Geo11365.Hidden11365"
    @test string(Plain11365.B()) == "Main.Plain11365.B()"
    @test string(typeof(Plain11365.B())) == "Main.Plain11365.B"
end

true
