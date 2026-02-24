# Test Array{T} as Pure Julia mutable struct wrapping Memory{T} (Issue #2760)
# This tests the struct-based Array definition, not the compiler-intercepted path.
# The struct is constructed directly with Memory{T} and a size tuple.
#
# Note: Methods like size(), length(), getindex(), setindex!() on the struct
# are not yet supported via dispatch due to type system limitations.
# This test verifies struct construction and direct field access.

using Test

@testset "Array{T} struct construction - Int64" begin
    # Create Memory{Int64} and construct Array struct
    mem = Memory{Int64}(3)
    mem[1] = 10
    mem[2] = 20
    mem[3] = 30
    a = Array{Int64}(mem, (3,))

    # Verify field access
    @test a._size == (3,)

    # Verify memory field holds correct data
    m = a._mem
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
    a = Array{Float64}(mem, (4,))

    @test a._size == (4,)
    m = a._mem
    @test m[1] == 1.5
    @test m[4] == 4.5
end

@testset "Array{T} struct mutability" begin
    mem = Memory{Int64}(3)
    mem[1] = 1
    mem[2] = 2
    mem[3] = 3
    a = Array{Int64}(mem, (3,))

    # Mutable struct: can change _size field
    a._size = (1, 3)
    @test a._size == (1, 3)

    # Memory mutation through field access
    m = a._mem
    m[2] = 99
    @test a._mem[2] == 99
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
    a = Array{Float64}(mem, (2, 3))

    @test a._size == (2, 3)
    @test a._mem[1] == 1.0
    @test a._mem[6] == 6.0

    # Verify size tuple dimensions
    s = a._size
    @test s[1] == 2
    @test s[2] == 3
end

@testset "Array{Bool} struct" begin
    mem = Memory{Bool}(3)
    mem[1] = true
    mem[2] = false
    mem[3] = true
    a = Array{Bool}(mem, (3,))

    @test a._size == (3,)
    m = a._mem
    @test m[1] == true
    @test m[2] == false
    @test m[3] == true
end

true
