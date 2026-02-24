# Test public keyword (Julia 1.11+)

using Test

@testset "Public keyword" begin
    # Since we don't have a module system yet, public statements are no-ops
    # They should parse and lower without errors

    # These should not throw errors
    public foo
    public bar, baz

    # Verify the code continues to execute
    @test 2 + 2 == 4
end

true
