# Issue #9198 S5: Complex{Float64} arrays fold onto the S4 general contiguous
# isbits `StructF64` storage (was the ComplexF64-specific `ArrayData::F64`
# interleaved buffer). This exercises construction / index get+set / iterate /
# collect / map / broadcast / matmul / reflection / display through the folded
# storage path. Behavior must stay byte-identical to upstream Julia.

using Test

@testset "construct + index get/set" begin
    a = [Complex(1.0, 2.0), Complex(3.0, 4.0), Complex(5.0, 6.0)]
    @test length(a) == 3
    @test a[1] == 1.0 + 2.0im
    @test real(a[2]) == 3.0
    @test imag(a[3]) == 6.0
    a[2] = 30.0 + 40.0im
    @test a[2] == 30.0 + 40.0im
    a[1] = 7.0            # real scalar stores as (7, 0)
    @test a[1] == 7.0 + 0.0im

    v = Vector{Complex{Float64}}(undef, 3)
    v[1] = 1.0 + 1.0im
    v[2] = 2.0 - 2.0im
    v[3] = -3.0 + 0.5im
    @test v[1] == 1.0 + 1.0im
    @test v[2] == 2.0 - 2.0im
    @test v[3] == -3.0 + 0.5im
end

@testset "zeros / fill / similar" begin
    z = zeros(Complex{Float64}, 4)
    @test all(x -> x == 0.0 + 0.0im, z)
    z[3] = 9.0 + 8.0im
    @test z[3] == 9.0 + 8.0im
    @test z[4] == 0.0 + 0.0im

    f = fill(2.0 + 3.0im, 3)
    @test f[1] == 2.0 + 3.0im
    @test f[3] == 2.0 + 3.0im

    s = similar(f)
    @test length(s) == 3
    s[1] = 1.0 + 0.0im
    @test s[1] == 1.0 + 0.0im
end

@testset "iterate / collect / map / reduce" begin
    a = [1.0 + 1.0im, 2.0 + 2.0im, 3.0 + 3.0im]
    acc = 0.0 + 0.0im
    for z in a
        acc += z
    end
    @test acc == 6.0 + 6.0im
    @test sum(a) == 6.0 + 6.0im
    @test collect(a) == a
    m = map(z -> z * 2, a)
    @test m[1] == 2.0 + 2.0im
    @test m[3] == 6.0 + 6.0im
    d = [conj(z) for z in a]
    @test d[2] == 2.0 - 2.0im
end

@testset "broadcast" begin
    a = [1.0 + 2.0im, 3.0 + 4.0im]
    b = [1.0 + 0.0im, 0.0 + 1.0im]
    @test (a .+ b)[1] == 2.0 + 2.0im
    @test (a .+ b)[2] == 3.0 + 5.0im
    @test (a .* 2)[1] == 2.0 + 4.0im
    @test conj.(a)[1] == 1.0 - 2.0im
    @test abs.(a)[1] == abs(1.0 + 2.0im)
end

@testset "matmul / scalar mul" begin
    A = [1.0+0.0im 2.0+0.0im; 0.0+1.0im 1.0+0.0im]
    x = [1.0+0.0im, 1.0+0.0im]
    y = A * x
    @test y[1] == 3.0 + 0.0im
    @test y[2] == 1.0 + 1.0im
    @test (2.0 * x)[1] == 2.0 + 0.0im
end

@testset "reflection + display" begin
    a = [1.0 + 2.0im, 3.0 + 4.0im]
    @test eltype(a) == Complex{Float64}
    @test typeof(a) == Vector{Complex{Float64}}
    @test sizeof(a) == 32
    @test sizeof(Vector{Complex{Float64}}(undef, 5)) == 80
    @test string(a) == "ComplexF64[1.0 + 2.0im, 3.0 + 4.0im]"
    M = [1.0+2.0im 3.0+4.0im; 5.0+6.0im 7.0+8.0im]
    @test string(M) == "ComplexF64[1.0 + 2.0im 3.0 + 4.0im; 5.0 + 6.0im 7.0 + 8.0im]"
end

true
