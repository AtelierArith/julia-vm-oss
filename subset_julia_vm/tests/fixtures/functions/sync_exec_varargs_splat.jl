using Test

# Test sync_exec CallWithSplat with varargs parameter binding (Issue #2213)
# The sync executor must collect excess splatted arguments into a Tuple
# for varargs parameters, matching the behavior of call.rs and call_dynamic.rs.

function sum_all(args...)
    s = 0
    for a in args
        s += a
    end
    s
end

function first_and_rest(x, rest...)
    (x, rest)
end

@testset "sync_exec CallWithSplat varargs" begin
    # Basic varargs splat from array
    arr = [1, 2, 3, 4, 5]
    @test sum_all(arr...) == 15

    # Varargs splat from tuple
    t = (10, 20, 30)
    @test sum_all(t...) == 60

    # Single element splat
    @test sum_all([42]...) == 42

    # Mixed fixed + varargs with splat
    arr2 = [1, 2, 3]
    result = first_and_rest(arr2...)
    @test result[1] == 1
    @test result[2] == (2, 3)

    # Two-element splat with fixed + varargs
    result2 = first_and_rest([10, 20]...)
    @test result2[1] == 10
    @test result2[2] == (20,)
end

true
