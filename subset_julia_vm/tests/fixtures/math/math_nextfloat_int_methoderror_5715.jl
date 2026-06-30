using Test

# Issue #5715: nextfloat/prevfloat are defined only for AbstractFloat. An Integer
# argument is a MethodError upstream, but sjulia leniently coerced it to Float64.
# Now the (post-#5690) runtime builtin raises a MethodError for non-float arguments.

@testset "nextfloat/prevfloat reject integer arguments (Issue #5715)" begin
    @test_throws MethodError nextfloat(1)
    @test_throws MethodError prevfloat(1)
    @test_throws MethodError nextfloat(true)
    @test_throws MethodError nextfloat(Int8(3))

    # AbstractFloat arguments still work (regression for #5690).
    @test nextfloat(1.0) == 1.0000000000000002
    @test prevfloat(2.0) < 2.0
    @test nextfloat(1.0f0) isa Float32
    @test prevfloat(1.0f0) isa Float32
    @test nextfloat(floatmax(Float32)) == Inf32
end

true
