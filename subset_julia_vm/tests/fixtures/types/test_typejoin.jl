# Test typejoin function - compute smallest common supertype
# typejoin(A, B) walks both supertype chains to find the first common ancestor

using Test

@testset "typejoin - smallest common supertype" begin
    # Same type returns itself
    @test typejoin(Int64, Int64) === Int64
    @test typejoin(Float64, Float64) === Float64
    @test typejoin(String, String) === String

    # Numeric type hierarchy
    @test typejoin(Int64, Float64) === Real
    @test typejoin(Int64, UInt64) === Integer
    @test typejoin(Bool, Int64) === Integer
    @test typejoin(Bool, UInt8) === Integer

    # Unrelated types -> Any
    @test typejoin(Int64, String) === Any
    @test typejoin(Float64, String) === Any

    # Any with anything -> Any
    @test typejoin(Any, Int64) === Any
    @test typejoin(Int64, Any) === Any
    @test typejoin(Any, Any) === Any
end

true  # Test passed
