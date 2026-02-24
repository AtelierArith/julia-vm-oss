# Test isabstracttype function

using Test

@testset "isabstracttype - check if type is abstract" begin

    # Abstract types return true
    @assert isabstracttype(Number)
    @assert isabstracttype(Real)
    @assert isabstracttype(Integer)
    @assert isabstracttype(Signed)
    @assert isabstracttype(Unsigned)
    @assert isabstracttype(AbstractFloat)
    @assert isabstracttype(AbstractString)
    @assert isabstracttype(Any)

    # Concrete types return false
    @assert !isabstracttype(Int64)
    @assert !isabstracttype(Float64)
    @assert !isabstracttype(Bool)
    @assert !isabstracttype(String)

    @test (true)
end

true  # Test passed
