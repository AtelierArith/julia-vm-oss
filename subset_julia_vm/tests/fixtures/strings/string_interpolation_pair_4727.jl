using Test

@testset "string interpolation of Pair does not leak StructRef (Issue #4727)" begin
    p = Pair(1, 2)
    @test "$p" == "1 => 2"
    @test "Wrapped: $p" == "Wrapped: 1 => 2"
    @test "$p, $p" == "1 => 2, 1 => 2"

    # Symbol field follows show semantics inside Pair
    @test "$(Pair(:x, 3.14))" == ":x => 3.14"
end

@testset "string interpolation resolves nested Pair inside Tuple (Issue #4727)" begin
    p = Pair(1, 2)
    @test "$((1, p))" == "(1, 1 => 2)"
    @test "$((p, p))" == "(1 => 2, 1 => 2)"
end

true
