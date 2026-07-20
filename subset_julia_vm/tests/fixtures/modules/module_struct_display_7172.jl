# Issue #7172: a struct defined inside a module, brought into scope via `using`,
# displays with its unqualified type name (`Point{Int64}`), matching upstream —
# not the module-qualified `Geometry.Point{Int64}` the VM stores internally.
using Test

module Geometry
export Point
struct Point{T}
    x::T
    y::T
end
end

using .Geometry

@testset "Issue #7172: in-scope module struct display is unqualified" begin
    @test string(Point(3, 4)) == "Point{Int64}(3, 4)"
    @test string(Point(1.5, 2.5)) == "Point{Float64}(1.5, 2.5)"
    # qualified constructor produces the same unqualified display
    @test string(Geometry.Point(7, 8)) == "Point{Int64}(7, 8)"
end

true
