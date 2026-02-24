# Test @allocated and @allocations macros (stub implementations)

using Test

@testset "@allocated and @allocations macros" begin
    # Test @allocated returns 0 (stub implementation)
    bytes = @allocated begin
        x = 0
        for i in 1:100
            x = x + i
        end
        x
    end
    @test bytes == 0  # Stub always returns 0

    # Test @allocations returns 0 (stub implementation)
    count = @allocations sum(1:100)
    @test count == 0  # Stub always returns 0

    # Test that the expression is evaluated even though allocation returns 0
    result = 0
    @allocated begin
        result = 42
    end
    @test result == 42
end

true  # Test passed
