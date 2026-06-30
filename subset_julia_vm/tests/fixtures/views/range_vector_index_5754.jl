using Test

# Issue #5754: indexing a range with a VECTOR of indices (or a Bool mask)
# materialized the selected elements into a Vector, but `(1:10)[[1, 3, 5]]`
# failed with "no method matching getindex(UnitRange{Int64}) with range index"
# because the runtime matcher does not route a native-array index to the
# pure-Julia getindex(::AbstractRange, ::AbstractVector). The IndexSlice handler
# now materializes the range and reuses the array fancy-index path; a range
# INDEX (`(1:10)[2:4]`) still returns a range.

@testset "range indexed by a vector of indices (Issue #5754)" begin
    @test (1:10)[[1, 3, 5]] == [1, 3, 5]
    @test (1:2:20)[[1, 3]] == [1, 5]
    @test (10:14)[[true, false, true, false, true]] == [10, 12, 14]
    @test (1.0:0.5:3.0)[[1, 3]] == [1.0, 2.0]
    r = 1:10
    i = [2, 4, 6]
    @test r[i] == [2, 4, 6]
    @test typeof((1:10)[[1, 3]]) == Vector{Int64}

    # A range INDEX (slice) still returns a range, not a materialized Vector
    @test (1:10)[2:4] === 2:4
    @test (1:10)[3] == 3
    # Reverse and out-of-order selections
    @test (1:10)[[5, 1, 3]] == [5, 1, 3]
end

true
