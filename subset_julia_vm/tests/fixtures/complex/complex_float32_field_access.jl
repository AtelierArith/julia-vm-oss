# Complex{Float32} field access test
# Related: Issue #1651 (bug), Issue #1652 (fix)
# Fixed: Field access on Complex{Float32} now returns Float32 instead of Float64

using Test

@testset "Complex{Float32} field access (Issue #1651)" begin
    # Create a Complex{Float32} value
    z = Complex{Float32}(Float32(1.5), Float32(2.5))

    # Access real part
    re = z.re
    @test re == Float32(1.5)
    @test typeof(re) === Float32

    # Access imaginary part
    im_val = z.im
    @test im_val == Float32(2.5)
    @test typeof(im_val) === Float32
end

@testset "Complex{Float64} field access (regression test)" begin
    # Ensure Complex{Float64} still works correctly
    z = Complex{Float64}(1.5, 2.5)

    # Access real part
    re = z.re
    @test re == 1.5
    @test typeof(re) === Float64

    # Access imaginary part
    im_val = z.im
    @test im_val == 2.5
    @test typeof(im_val) === Float64
end

true
