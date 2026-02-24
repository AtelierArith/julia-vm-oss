# Test isnumeric function - Unicode numeric character check

using Test

@testset "isnumeric(c) - check if character is numeric (Unicode)" begin

    # === ASCII digits ===
    @assert isnumeric('0')
    @assert isnumeric('5')
    @assert isnumeric('9')

    # === Non-numeric ASCII ===
    @assert !isnumeric('a')
    @assert !isnumeric('Z')
    @assert !isnumeric(' ')
    @assert !isnumeric('!')

    # === Letters vs digits ===
    @assert !isnumeric('A')
    @assert isnumeric('1')

    # All tests passed
    @test (true)
end

true  # Test passed
