# Constant-step integer range for-loops (Issue #5166).
#
# When the step of an integer `for` range loop is a compile-time constant, the
# compiler hoists the per-iteration sign check out of the loop and specializes the
# increment. This fixture pins the observable behavior across positive/negative,
# unit/non-unit, and empty-range cases so the optimization stays parity-exact with
# upstream Julia.

using Test

@testset "const-step for loops (Issue #5166)" begin
    # 1:n — implicit unit step.
    acc1 = Int64[]
    for i in 1:5
        push!(acc1, i)
    end
    @test acc1 == [1, 2, 3, 4, 5]

    # n:-1:1 — literal negative unit step counts down.
    acc2 = Int64[]
    for i in 5:-1:1
        push!(acc2, i)
    end
    @test acc2 == [5, 4, 3, 2, 1]

    # 1:2:n — constant non-unit positive step.
    acc3 = Int64[]
    for i in 1:2:7
        push!(acc3, i)
    end
    @test acc3 == [1, 3, 5, 7]

    # Non-unit step that overshoots the stop and never lands on it exactly.
    acc4 = Int64[]
    for i in 1:2:6
        push!(acc4, i)
    end
    @test acc4 == [1, 3, 5]

    # Negative non-unit step.
    acc5 = Int64[]
    for i in 10:-3:1
        push!(acc5, i)
    end
    @test acc5 == [10, 7, 4, 1]

    # Empty range: start > stop with a positive step yields zero iterations.
    count1 = 0
    for i in 5:1
        count1 += 1
    end
    @test count1 == 0

    # Empty range: start < stop with a negative step yields zero iterations.
    count2 = 0
    for i in 1:-1:5
        count2 += 1
    end
    @test count2 == 0

    # Single-element range.
    acc6 = Int64[]
    for i in 3:3
        push!(acc6, i)
    end
    @test acc6 == [3]

    # Loop variable type is Int64 throughout.
    for i in 1:3
        @test typeof(i) == Int64
    end
    for i in 3:-1:1
        @test typeof(i) == Int64
    end

    # Sum via constant unit step.
    s1 = 0
    for i in 1:100
        s1 += i
    end
    @test s1 == 5050

    # Sum via constant step of 2.
    s2 = 0
    for i in 2:2:100
        s2 += i
    end
    @test s2 == 2550

    # break / continue interact correctly with the specialized increment.
    acc7 = Int64[]
    for i in 1:10
        if i == 4
            continue
        end
        if i == 7
            break
        end
        push!(acc7, i)
    end
    @test acc7 == [1, 2, 3, 5, 6]

    # Variable endpoint with a constant positive step.
    n1 = 6
    acc8 = Int64[]
    for i in 1:2:n1
        push!(acc8, i)
    end
    @test acc8 == [1, 3, 5]

    # Variable start with a constant negative non-unit step.
    n2 = 6
    acc9 = Int64[]
    for i in n2:-2:1
        push!(acc9, i)
    end
    @test acc9 == [6, 4, 2]
end

true
