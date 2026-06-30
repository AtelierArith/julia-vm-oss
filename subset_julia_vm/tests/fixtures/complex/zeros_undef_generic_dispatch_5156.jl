# Complex zeros/undef via generic dispatch (Issue #5156)
#
# `zeros(Complex{Float64}, dims...)` and `Array{Complex{Float64}}(undef, dims...)`
# must route through the generic pure-Julia `zeros(::Type{T}, ...)` /
# `_array_undef_from_dims(::Type{T}, ...)` dispatch, NOT a Complex-specialized
# method. After removing the dedicated Rust builtins
# (ZerosComplexF64 / AllocUndefComplexF64) and the redundant pure-Julia
# Complex-specialized `zeros`/`_array_undef_from_dims` methods, these operations
# keep working with full read/write/eltype/size semantics through the generic
# path, preserving the interleaved (re,im) storage optimization.

using Test

@testset "zeros(Complex{Float64}, n) generic dispatch" begin
    z = zeros(Complex{Float64}, 3)
    @test length(z) == 3
    @test eltype(z) == Complex{Float64}
    @test z[1] == Complex(0.0, 0.0)
    @test z[2] == Complex(0.0, 0.0)
    @test z[3] == Complex(0.0, 0.0)

    # Mutate through the generic Complex storage path
    z[2] = Complex(3.0, 4.0)
    @test real(z[2]) == 3.0
    @test imag(z[2]) == 4.0
    # Untouched elements stay zero
    @test z[1] == Complex(0.0, 0.0)
    @test z[3] == Complex(0.0, 0.0)
end

@testset "zeros(Complex{Float64}, m, n) matrix generic dispatch" begin
    m = zeros(Complex{Float64}, 2, 2)
    @test size(m) == (2, 2)
    @test eltype(m) == Complex{Float64}
    @test m[1, 1] == Complex(0.0, 0.0)
    m[1, 2] = Complex(5.0, 6.0)
    @test real(m[1, 2]) == 5.0
    @test imag(m[1, 2]) == 6.0
    @test m[2, 1] == Complex(0.0, 0.0)
end

@testset "Array{Complex{Float64}}(undef, n) generic dispatch" begin
    a = Array{Complex{Float64}}(undef, 3)
    @test length(a) == 3
    @test eltype(a) == Complex{Float64}

    a[1] = Complex(1.0, 2.0)
    a[2] = Complex(3.0, 4.0)
    a[3] = Complex(5.0, 6.0)

    @test real(a[1]) == 1.0
    @test imag(a[1]) == 2.0
    @test real(a[2]) == 3.0
    @test imag(a[2]) == 4.0
    @test real(a[3]) == 5.0
    @test imag(a[3]) == 6.0
end

@testset "Vector{Complex{Float64}}(undef, n) generic dispatch" begin
    v = Vector{Complex{Float64}}(undef, 2)
    @test length(v) == 2
    @test eltype(v) == Complex{Float64}
    v[1] = Complex(10.0, 20.0)
    v[2] = Complex(30.0, 40.0)
    @test v[1] == Complex(10.0, 20.0)
    @test v[2] == Complex(30.0, 40.0)
end

@testset "zeros(Complex{Float64}, n) via tuple dims generic dispatch" begin
    z = zeros(Complex{Float64}, (2,))
    @test length(z) == 2
    @test eltype(z) == Complex{Float64}
    @test z[1] == Complex(0.0, 0.0)
    @test z[2] == Complex(0.0, 0.0)
end

true
