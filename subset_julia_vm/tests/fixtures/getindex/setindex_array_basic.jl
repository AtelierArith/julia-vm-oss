# Test: setindex! on Array modifies in place

using Test

@testset "setindex!(arr, v, i) modifies array in place" begin
    arr = [1.0, 2.0, 3.0]

    # setindex!(arr, value, index) modifies arr at index
    setindex!(arr, 10.0, 2)
    @assert arr[2] == 10.0

    # Verify other elements unchanged
    @assert arr[1] == 1.0
    @assert arr[3] == 3.0

    @test (true)
end

true  # Test passed
