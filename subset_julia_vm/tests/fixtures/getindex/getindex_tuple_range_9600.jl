using Test

# Issue #9600: tuple indexing with a range must return a tuple slice. The
# compiler used to coerce the range index to Int64 before the VM could handle it.
@testset "tuple range indexing (Issue #9600)" begin
    t = (10, "b", 3.5, :d)

    @test t[1:2] == (10, "b")
    @test t[2:4] == ("b", 3.5, :d)
    @test t[1:1] == (10,)
    @test t[3:2] == ()
    @test t[1:2:4] == (10, 3.5)
    @test t[4:-2:1] == (:d, "b")
    @test t[:] == t

    r = 2:3
    @test t[r] == ("b", 3.5)
    @test getindex(t, 1:2) == (10, "b")
    @test t[Base.OneTo(2)] == (10, "b")

    @test_throws BoundsError t[0:1]
    @test_throws BoundsError t[3:5]
end

true
