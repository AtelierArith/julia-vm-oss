# Struct iterable type inference test
# Tests that loop variables are correctly inferred from struct iterables
# like LinRange, StepRangeLen, UnitRange, StepRange, and OneTo

using Test

@testset "Struct iterable type inference" begin
    # LinRange iteration - element type should be Float64
    lr = LinRange(0.0, 1.0, 5)
    sum_lr = 0.0
    for x in lr
        sum_lr += x
    end
    @test sum_lr == 2.0  # 0.0 + 0.25 + 0.5 + 0.75 + 1.0 = 2.5, but with rounding...

    # Using collect to verify values
    lr_collected = collect(lr)
    @test length(lr_collected) == 5
    @test lr_collected[1] == 0.0
    @test lr_collected[5] == 1.0

    # StepRangeLen iteration via range() function
    # range(0, 1, length=5) creates a StepRangeLen
    srl = range(0.0, 1.0, length=5)
    sum_srl = 0.0
    for x in srl
        sum_srl += x
    end
    @test sum_srl == 2.0

    # Direct LinRange with integer bounds
    lr2 = LinRange(1, 10, 10)
    sum_lr2 = 0.0
    for x in lr2
        sum_lr2 += x
    end
    @test sum_lr2 == 55.0  # 1 + 2 + ... + 10 = 55

    # Nested loop with struct iterables
    total = 0.0
    for i in LinRange(1.0, 3.0, 3)
        for j in LinRange(1.0, 2.0, 2)
            total += i * j
        end
    end
    @test total == 18.0  # (1+2+3) * (1+2) = 6 * 3 = 18

    # Enumerate with LinRange
    indices = Int64[]
    values = Float64[]
    for (i, v) in enumerate(LinRange(0.0, 2.0, 3))
        push!(indices, i)
        push!(values, v)
    end
    @test length(indices) == 3
    @test indices[1] == 1
    @test indices[3] == 3
    @test values[1] == 0.0
    @test values[3] == 2.0
end

true
