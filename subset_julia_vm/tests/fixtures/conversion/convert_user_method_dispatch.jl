using Test

struct Celsius
    val::Float64
end

function Base.convert(::Type{Celsius}, x::Real)
    Celsius(Float64(x))
end

@testset "conversion_convert_user_method_dispatch: user convert method dispatch" begin
    c = convert(Celsius, 20)
    @test c.val == 20.0

    # No Pure Julia UInt64 method exists for this pair; this must still use the
    # Rust fallback instead of the generic convert(::Type{T}, x::T) identity.
    u = convert(UInt64, 7)
    @test typeof(u) == UInt64
    @test u == UInt64(7)
end

true
