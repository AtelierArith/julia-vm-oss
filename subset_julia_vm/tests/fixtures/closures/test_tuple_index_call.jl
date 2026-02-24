using Test

# Test calling closures via tuple indexing (Issue #2240).
# Previously, tuple[i]() failed with UnsupportedCallTarget lowering error.

function make_pair()
    function get_a()
        1
    end
    function get_b()
        2
    end
    (get_a, get_b)
end

pair = make_pair()

@testset "tuple index call target" begin
    @test pair[1]() == 1
    @test pair[2]() == 2
end

# Test with arguments
function make_ops()
    function add(x, y)
        x + y
    end
    function mul(x, y)
        x * y
    end
    (add, mul)
end

ops = make_ops()

@testset "tuple index call with arguments" begin
    @test ops[1](3, 4) == 7
    @test ops[2](3, 4) == 12
end

true
