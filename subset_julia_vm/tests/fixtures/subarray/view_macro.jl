# Test @view macro
# @view A[i:j] should transform to view(A, i:j)

using Test

@testset "@view macro basic (Issue #320)" begin
    # Create an array
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]

    # Create a view using @view macro
    v = @view arr[2:4]

    # Check view indexing
    @test v[1] == 2.0
    @test v[2] == 3.0
    @test v[3] == 4.0

    # Test length
    @test length(v) == 3
end

true
