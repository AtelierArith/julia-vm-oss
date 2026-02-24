# Loop element type inference test
# Tests that loop variables are correctly inferred from iterables

using Test

@testset "Loop element type inference" begin
    # Array iteration - element type should be inferred as Int64
    arr = [1, 2, 3]
    total = 0
    for x in arr
        total += x
    end
    @test total == 6

    # Float array iteration - element type should be Float64
    farr = [1.0, 2.0, 3.0]
    ftotal = 0.0
    for x in farr
        ftotal += x
    end
    @test ftotal == 6.0

    # Tuple iteration
    t = (1, 2, 3)
    sum_t = 0
    for x in t
        sum_t += x
    end
    @test sum_t == 6

    # Range iteration - element type should be Int
    sum_range = 0
    for i in 1:5
        sum_range += i
    end
    @test sum_range == 15

    # Nested loops
    result = 0
    for i in 1:3
        for j in [10, 20, 30]
            result += i * j
        end
    end
    @test result == 360  # (1+2+3) * (10+20+30)

    # String iteration - element type should be Char
    s = "abc"
    chars = Char[]
    for c in s
        push!(chars, c)
    end
    @test length(chars) == 3
    # Char == Char comparison is not yet supported (see issue #945)
    # Using Int conversion as workaround
    @test Int(chars[1]) == Int('a')
end

true
