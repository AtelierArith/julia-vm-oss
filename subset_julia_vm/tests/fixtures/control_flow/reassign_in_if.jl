# Variable reassignment inside if block
# This tests that variables defined in loop body can be reassigned inside if blocks

using Test

function test_reassign()
    result = 0.0
    for i in 1:3
        z = 1 + floor(i * 1.5)
        if z > 3
            z = 3
        end
        result = result + z
    end
    result  # 2 + 3 + 3 = 8
end

@testset "Variable reassignment inside if block (regression test)" begin
    @test (test_reassign()) == 8.0
end

true  # Test passed
