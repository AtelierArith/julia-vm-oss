# isequal function - NaN-aware equality
# isequal(1, 1.0) is true, isequal(NaN, NaN) is true, isequal(-0.0, 0.0) is false
# Result: 1 + 1 + 0 = 2.0

using Test

@testset "isequal function - NaN-aware equality" begin
    result = 0.0
    result += isequal(1, 1.0) ? 1.0 : 0.0
    result += isequal(NaN, NaN) ? 1.0 : 0.0
    result += isequal(-0.0, 0.0) ? 1.0 : 0.0
    @test (result) == 2.0
end

true  # Test passed
