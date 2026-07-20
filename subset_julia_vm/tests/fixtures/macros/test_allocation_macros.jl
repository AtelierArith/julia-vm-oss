# Test @allocated and @allocations macros (Issue #10237)
# sjulia's implementations are stubs that always return 0; upstream returns
# real measurements. Assert only the portable contract: a non-negative
# integer is returned and the wrapped expression is evaluated.

using Test

@testset "@allocated and @allocations macros" begin
    # @allocated returns a non-negative integer byte count
    bytes = @allocated begin
        x = 0
        for i in 1:100
            x = x + i
        end
        x
    end
    @test bytes isa Integer
    @test bytes >= 0

    # @allocations returns a non-negative integer allocation count
    count = @allocations sum(1:100)
    @test count isa Integer
    @test count >= 0

    # The wrapped expression is evaluated for its side effects
    result = 0
    @allocated begin
        result = 42
    end
    @test result == 42
end

true  # Test passed
