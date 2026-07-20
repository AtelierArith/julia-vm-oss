# Issue #10385: println/show/string of a rank>=3 Array previously leaked the
# internal representation ("Array{Float64, 3}(MemoryRef{...}, dims)" for
# print/string, "Array{T, N} with size (...)" for show) instead of upstream's
# nested ;;-literal compact form. The compact renderer now recurses: dim-1
# entries join with "; ", dim-2 columns with a space, and dim-k slices
# (k >= 3) with k semicolons plus a space; higher-rank EMPTY arrays print the
# undef-constructor form.

using Test

@testset "rank>=3 array display matches upstream (Issue #10385)" begin
    @test string(zeros(2, 2, 2)) == "[0.0 0.0; 0.0 0.0;;; 0.0 0.0; 0.0 0.0]"
    @test string(reshape(1:4, 2, 1, 2)) == "[1; 2;;; 3; 4]"
    @test string(reshape(1:16, 2, 2, 2, 2)) ==
          "[1 3; 2 4;;; 5 7; 6 8;;;; 9 11; 10 12;;; 13 15; 14 16]"
    @test string(zeros(1, 1, 2)) == "[0.0;;; 0.0]"
    @test string(fill(true, 2, 2, 2)) == "Bool[1 1; 1 1;;; 1 1; 1 1]"
    @test string(zeros(2, 0, 2)) == "Array{Float64, 3}(undef, 2, 0, 2)"
    @test sprint(show, zeros(2, 2, 2)) == "[0.0 0.0; 0.0 0.0;;; 0.0 0.0; 0.0 0.0]"
    # Rank 1/2 forms unchanged.
    @test string(zeros(2, 2)) == "[0.0 0.0; 0.0 0.0]"
    @test string(zeros(2)) == "[0.0, 0.0]"
end

true
