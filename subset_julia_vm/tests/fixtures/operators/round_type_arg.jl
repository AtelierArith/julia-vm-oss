# round(T, x) / floor(T, x) / ceil(T, x) / trunc(T, x) - rounding with type argument (Issue #2028)
# The two-argument form applies the rounding operation then converts to the target type.

using Test

@testset "round/floor/ceil/trunc with type argument (Issue #2028)" begin
    # round(Int, x)
    @test round(Int, 3.7) == 4
    @test round(Int, 3.2) == 3
    @test round(Int, -2.3) == -2
    @test round(Int, -2.7) == -3
    @test round(Int, 0.5) == 0     # banker's rounding: round to even
    @test round(Int, 1.5) == 2     # banker's rounding: round to even

    # floor(Int, x)
    @test floor(Int, 3.7) == 3
    @test floor(Int, 3.2) == 3
    @test floor(Int, -2.3) == -3
    @test floor(Int, -2.7) == -3

    # ceil(Int, x)
    @test ceil(Int, 3.7) == 4
    @test ceil(Int, 3.2) == 4
    @test ceil(Int, -2.3) == -2
    @test ceil(Int, -2.7) == -2

    # trunc(Int, x)
    @test trunc(Int, 3.7) == 3
    @test trunc(Int, -3.7) == -3
    @test trunc(Int, 3.2) == 3
    @test trunc(Int, -3.2) == -3

    # Using Int64 explicitly
    @test round(Int64, 5.5) == 6
    @test floor(Int64, 5.9) == 5
    @test ceil(Int64, 5.1) == 6
    @test trunc(Int64, 5.9) == 5

    # Single-argument forms still work
    @test round(3.7) == 4.0
    @test floor(3.7) == 3.0
    @test ceil(3.2) == 4.0
    @test trunc(3.7) == 3.0
end

true
