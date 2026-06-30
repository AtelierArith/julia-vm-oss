# Issue #5137: broadcasting over a SubArray view (`view(v, a:b) .+ 1`) now works.
# Previously the pure-Julia broadcast pipeline treated a view as a 0-dimensional
# scalar (it only recognized native `Array`), so `v .+ 10` fell through to a
# scalar `+(::SubArray, ::Int)` and raised. The view now participates in the
# broadcast machinery as the array it aliases: shape/axes, element-type
# inference, the Extruded loop, the non-Extruded (BitVector destination) path,
# and the scalar / 2-D fast-path guards all recognize a SubArray.

using Test

@testset "broadcast over SubArray (Issue #5137)" begin
    # 1-D arithmetic broadcasts (view ⊕ scalar, scalar ⊕ view, view ⊕ view)
    v = view([1, 2, 3, 4], 2:4)
    @test v .+ 10 == [12, 13, 14]
    @test v .- 1 == [1, 2, 3]
    @test v .* 2 == [4, 6, 8]
    @test 100 .+ v == [102, 103, 104]
    @test v .* v == [4, 9, 16]
    @test v .+ [10, 20, 30] == [12, 23, 34]

    # Float division and power
    fv = view([10.0, 20.0, 30.0], 1:2)
    @test fv ./ 2 == [5.0, 10.0]
    @test view([1, 2, 3], 1:2) .^ 2 == [1, 4]

    # Math functions broadcast over a view
    @test sqrt.(view([1.0, 4.0, 9.0], 1:3)) == [1.0, 2.0, 3.0]
    @test abs.(view([-1, -2, 3], 1:3)) == [1, 2, 3]

    # Comparison broadcasts produce a Bool result over a view
    cv = view([1, 2, 3, 4], 1:4)
    @test (cv .> 2) == [false, false, true, true]
    @test (cv .< 3) == [true, true, false, false]
    @test (view([1, 2, 2, 4], 1:4) .== 2) == [false, true, true, false]

    # 2-D view broadcasts preserve the matrix shape
    A = [1 2; 3 4]
    m = view(A, 1:2, 1:2)
    ms = m .+ 1
    @test ms == [2 3; 4 5]
    @test size(ms) == (2, 2)
    mm = m .+ [10 20; 30 40]
    @test mm == [11 22; 33 44]
    @test size(mm) == (2, 2)

    # dimension-dropping view broadcasts as a 1-D vector
    B = [1 2 3; 4 5 6]
    @test view(B, 1, :) .+ 100 == [101, 102, 103]

    # the parents are untouched by these (copying) broadcasts
    @test A == [1 2; 3 4]
    @test B == [1 2 3; 4 5 6]
end

true
