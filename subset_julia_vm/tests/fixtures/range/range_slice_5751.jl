using Test

# Issue #5751: indexing a range with a range index returns a sub-range, preserving
# the step and type. Previously errored "getindex(UnitRange) with range index".

@testset "range slicing with a range index (Issue #5751)" begin
    # UnitRange slices stay UnitRange
    @test (1:10)[1:3] == 1:3
    @test (1:10)[1:3] isa UnitRange{Int64}
    @test (1:10)[2:4] == 2:4
    @test (1:100)[5:10] == 5:10

    # StepRange slices preserve the (composed) step
    @test (1:2:20)[1:3] == 1:2:5
    @test (1:2:20)[1:3] isa StepRange{Int64,Int64}

    # Reverse index gives a descending range
    @test (1:10)[3:-1:1] == 3:-1:1

    # Empty slice
    @test isempty((10:20)[1:0])

    # Single-element slice
    @test (5:5)[1:1] == 5:5

    # Float ranges
    @test (0.0:0.5:5.0)[1:3] == 0.0:0.5:1.0

    # Inline consumers compile correctly (the result is inferred as a range)
    @test collect((1:10)[1:3]) == [1, 2, 3]
    @test sum((1:10)[1:3]) == 6
    @test length((1:10)[2:2:8]) == 4

    # first(range, n) now works (it slices r[1:n]) — was blocked by this gap
    @test first(1:10, 3) == 1:3
    @test first(1:2:20, 3) == 1:2:5
    @test first(1:10, 3) isa UnitRange{Int64}

    # Scalar indexing is unchanged (regression guard)
    @test (1:10)[3] == 3
    @test (1:2:20)[4] == 7
end

true
