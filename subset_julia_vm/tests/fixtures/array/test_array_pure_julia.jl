# Test Array{T,N} as Pure Julia mutable struct wrapping MemoryRef{T} (Issues #2760/#6648)
# This tests the struct-based Array definition, not the compiler-intercepted path.
# The struct is constructed directly with MemoryRef{T} and a size tuple.
#
# This verifies both direct field access and Pure Julia wrapper methods that
# delegate shape/indexing/mutation to the backing Memory{T}.

using Test

@testset "Array{T} struct construction - Int64" begin
    # Create Memory{Int64} and construct Array struct
    mem = Memory{Int64}(3)
    mem[1] = 10
    mem[2] = 20
    mem[3] = 30
    a = Array{Int64,1}(memoryref(mem), (3,))

    # Verify field access
    @test a.size == (3,)
    @test memoryindex(a.ref) == 1
    @test size(a) == (3,)
    @test size(a, 1) == 3
    @test length(a) == 3
    @test ndims(a) == 1
    @test a[1] == 10
    @test a[2] == 20
    @test a[3] == 30

    # Verify memory field holds correct data
    m = parent(a.ref)
    @test m[1] == 10
    @test m[2] == 20
    @test m[3] == 30
end

@testset "Array{T} struct construction - Float64" begin
    mem = Memory{Float64}(4)
    mem[1] = 1.5
    mem[2] = 2.5
    mem[3] = 3.5
    mem[4] = 4.5
    a = Array{Float64,1}(memoryref(mem), (4,))

    @test a.size == (4,)
    m = parent(a.ref)
    @test m[1] == 1.5
    @test m[4] == 4.5
end

@testset "Array{T} struct mutability" begin
    mem = Memory{Int64}(3)
    mem[1] = 1
    mem[2] = 2
    mem[3] = 3
    a = Array{Int64,1}(memoryref(mem), (3,))

    # Mutable struct: can change size field.
    a.size = (1, 3)
    @test a.size == (1, 3)
    @test size(a) == (1, 3)
    @test ndims(a) == 2

    # Memory mutation through field access
    m = parent(a.ref)
    m[2] = 99
    @test parent(a.ref)[2] == 99
    @test a[2] == 99

    a[3] = 123
    @test parent(a.ref)[3] == 123
    @test a[3] == 123
end

@testset "Array{T} struct 2D" begin
    # 2D array: 2x3 matrix (6 elements in column-major Memory)
    mem = Memory{Float64}(6)
    mem[1] = 1.0
    mem[2] = 2.0
    mem[3] = 3.0
    mem[4] = 4.0
    mem[5] = 5.0
    mem[6] = 6.0
    a = Array{Float64,2}(memoryref(mem), (2, 3))

    @test a.size == (2, 3)
    @test size(a) == (2, 3)
    @test size(a, 1) == 2
    @test size(a, 2) == 3
    @test size(a, 3) == 1
    @test length(a) == 6
    @test ndims(a) == 2
    @test parent(a.ref)[1] == 1.0
    @test parent(a.ref)[6] == 6.0
    @test a[1, 1] == 1.0
    @test a[2, 1] == 2.0
    @test a[1, 2] == 3.0
    @test a[2, 3] == 6.0

    a[1, 3] = 9.5
    @test parent(a.ref)[5] == 9.5
    @test a[1, 3] == 9.5

    # Verify size tuple dimensions
    s = a.size
    @test s[1] == 2
    @test s[2] == 3
end

@testset "wrap Array from Memory" begin
    mem = Memory{Int64}(4)
    mem[1] = 10
    mem[2] = 20
    mem[3] = 30
    mem[4] = 40

    a = wrap(Array, mem, (2, 2))
    @test size(a) == (2, 2)
    @test length(a) == 4
    @test ndims(a) == 2
    @test a[1, 1] == 10
    @test a[2, 1] == 20
    @test a[1, 2] == 30
    @test a[2, 2] == 40

    a[1, 2] = 99
    @test mem[3] == 99

    mem[4] = 77
    @test a[2, 2] == 77

    v = wrap(Array, mem, 3)
    @test size(v) == (3,)
    @test size(v, 2) == 1
    @test length(v) == 3
    @test v[3] == 99

    full = wrap(Array, mem)
    @test size(full) == (4,)
    @test size(full, 2) == 1
    @test length(full) == 4
    @test full[4] == 77

    @test_throws DimensionMismatch wrap(Array, mem, (3, 2))
    @test_throws BoundsError v[4]
end

@testset "Array{T} struct 3D indexing" begin
    mem = Memory{Int64}(8)
    for i in 1:8
        mem[i] = i
    end

    a = wrap(Array, mem, (2, 2, 2))
    @test size(a) == (2, 2, 2)
    @test length(a) == 8
    @test ndims(a) == 3
    @test a[1, 1, 1] == 1
    @test a[2, 1, 1] == 2
    @test a[1, 2, 1] == 3
    @test a[2, 2, 1] == 4
    @test a[1, 1, 2] == 5
    @test a[2, 2, 2] == 8

    a[1, 2, 2] = 99
    @test mem[7] == 99
    @test a[1, 2, 2] == 99

    @test_throws BoundsError a[3, 1, 1]
    @test_throws BoundsError a[1, 1]
end

@testset "Array{T} struct reshape shares Memory" begin
    mem = Memory{Int64}(6)
    for i in 1:6
        mem[i] = i
    end

    a = wrap(Array, mem, (2, 3))
    v = reshape(a, 6)
    @test size(v) == (6,)
    @test length(v) == 6
    @test v[5] == 5

    v[6] = 99
    @test mem[6] == 99
    @test a[2, 3] == 99

    b = reshape(v, 3, 2)
    @test size(b) == (3, 2)
    @test b[3, 2] == 99

    r = wrap(Array, memoryref(mem, 2), (2, 2))
    rr = reshape(r, 4)
    @test size(rr) == (4,)
    @test size(rr, 2) == 1
    @test rr[1] == 2
    @test rr[4] == 5

    rr[4] = 77
    @test mem[5] == 77
    @test r[2, 2] == 77

    @test_throws DimensionMismatch reshape(a, 5)
end

@testset "Array{T} struct similar allocates Memory-backed wrapper" begin
    mem = Memory{Int64}(6)
    for i in 1:6
        mem[i] = i
    end

    a = wrap(Array, mem, (2, 3))
    b = similar(a)
    @test size(b) == (2, 3)
    @test length(b) == 6
    @test ndims(b) == 2

    b[2, 3] = 42
    @test b[2, 3] == 42
    @test mem[6] == 6

    v = similar(a, 4)
    @test size(v) == (4,)
    @test length(v) == 4
    v[4] = 77
    @test v[4] == 77

    m = similar(a, 3, 2)
    @test size(m) == (3, 2)
    @test length(m) == 6
    m[3, 2] = 99
    @test m[3, 2] == 99

    tf = similar(a, Float64, 2, 2)
    @test size(tf) == (2, 2)
    @test length(tf) == 4
    tf[2, 2] = 1.25
    @test tf[2, 2] == 1.25
    @test typeof(tf[2, 2]) == Float64

    tb = similar(a, Bool)
    @test size(tb) == (2, 3)
    tb[1, 1] = true
    @test tb[1, 1] == true
    @test typeof(tb[1, 1]) == Bool

    r = wrap(Array, memoryref(mem, 2), (2, 2))
    rr = similar(r)
    @test size(rr) == (2, 2)
    rr[2, 2] = 55
    @test rr[2, 2] == 55
    @test r[2, 2] == 5

    @test_throws DimensionMismatch similar(a, -1)
end

@testset "Array{Bool} struct" begin
    mem = Memory{Bool}(3)
    mem[1] = true
    mem[2] = false
    mem[3] = true
    a = Array{Bool,1}(memoryref(mem), (3,))

    @test a.size == (3,)
    m = parent(a.ref)
    @test m[1] == true
    @test m[2] == false
    @test m[3] == true
end

true
