using Test

function f_break_only_4684(n)
    for i in 1:n
        break
    end
    "done"
end

function f_break_with_assign_4684(n)
    x = 0
    for i in 1:n
        x = 1
        break
    end
    x
end

function f_break_string_4684(n)
    x = 1
    for i in 1:n
        x = "s"
        break
    end
    x
end

function f_continue_dynamic_4684(n)
    s = 0
    for i in 1:n
        if i == 2
            continue
        end
        s = s + i
    end
    s
end

function f_break_in_if_dynamic_4684(n, thresh)
    last = 0
    for i in 1:n
        if i > thresh
            break
        end
        last = i
    end
    last
end

function f_nested_break_dynamic_4684(m, n)
    s = 0
    for i in 1:m
        for j in 1:n
            if j == 2
                break
            end
            s = s + 1
        end
    end
    s
end

@testset "for-loop break/continue over parametric range terminates (Issue #4684)" begin
    # Top-level break in `for i in 1:n` must terminate after one
    # iteration. Before the fix, the specializer left the break's
    # placeholder `Jump(0)` unpatched, which after `entry_point`
    # relocation jumped back to the start of the specialized function
    # and caused an infinite loop. The reproduction never returned and
    # had to be killed.
    @test f_break_only_4684(3) == "done"
    @test f_break_only_4684(0) == "done"
    @test f_break_only_4684(1) == "done"
    @test f_break_only_4684(1000000) == "done"

    @test f_break_with_assign_4684(3) == 1
    @test f_break_with_assign_4684(0) == 0
    @test f_break_with_assign_4684(5) == 1

    @test f_break_string_4684(3) === "s"

    # Continue must jump to the increment+back-edge (continue target),
    # not back to the loop start (which would skip the increment and
    # also hang). Sum 1..n excluding 2 = n*(n+1)/2 - 2.
    @test f_continue_dynamic_4684(5) == (15 - 2)
    @test f_continue_dynamic_4684(1) == 1
    @test f_continue_dynamic_4684(2) == 1

    # Conditional break inside if — `last` retains the value just
    # before the iteration that broke.
    @test f_break_in_if_dynamic_4684(10, 3) == 3
    @test f_break_in_if_dynamic_4684(10, 0) == 0

    # Nested loops: inner break must target the inner loop, not the
    # outer. Without the per-loop save/restore around
    # break_positions / continue_positions, an inner break would patch
    # the outer loop's exit position.
    @test f_nested_break_dynamic_4684(3, 5) == 3
    @test f_nested_break_dynamic_4684(0, 5) == 0
end

true
