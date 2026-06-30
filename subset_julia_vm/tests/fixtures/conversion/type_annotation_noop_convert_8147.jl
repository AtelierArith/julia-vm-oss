using Test

# No-op cases: value already matches the declared concrete type. The
# `convert(T, x::T) = x` identity must be preserved while the hot-path
# `Convert` is elided (Issue #8147).
function gcd_return(a::Int, b::Int)::Int
    while b != 0
        tmp::Int = b
        b = a % b
        a = tmp
    end
    a
end

# Real conversions: the declared type differs from the value type, so the
# conversion must still happen (elision must not swallow genuine converts).
function as_float(x::Int)::Float64
    x
end

function typed_local_widens(x::Int)
    y::Float64 = x
    y
end

@testset "conversion_type_annotation_noop_convert_8147" begin
    # No-op convert keeps identity semantics and correct results.
    @test gcd_return(12, 18) == 6
    @test gcd_return(48, 36) == 12
    @test gcd_return(17, 5) == 1

    # Genuine conversions are preserved with the right value and type.
    @test as_float(7) === 7.0
    @test typeof(as_float(7)) == Float64
    @test typed_local_widens(3) === 3.0
    @test typeof(typed_local_widens(3)) == Float64
end

true
