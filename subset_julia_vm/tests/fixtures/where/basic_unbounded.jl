# Test basic where clause with unbounded type parameter

using Test

function identity(x::T) where T
    x
end

@testset "Function with unbounded where clause (where T)" begin
    @test (identity(10)) == 10.0
end

true  # Test passed
