# Test zeros and ones with type parameter (Issue #589, Issue #4036, Issue #4039, Issue #4044)
# zeros(Type, dims...) and ones(Type, dims...) should create typed arrays
# through Pure Julia allocation dispatch matching Julia's base/array.jl.

using Test

@testset "zeros(Type, dims...) and ones(Type, dims...) create typed arrays (Issue #589, Issue #4036, Issue #4039, Issue #4044)" begin

    # Test 1: zeros(Float64, n) - should create Float64 array (same as zeros(n))
    arr1 = zeros(Float64, 3)
    @test typeof(arr1) == Vector{Float64}
    @test eltype(arr1) == Float64
    @test length(arr1) == 3
    @test arr1[1] == 0.0
    @test arr1[2] == 0.0
    @test arr1[3] == 0.0

    # Test 2: zeros(Int64, n) - should create Int64 array of zeros
    arr2 = zeros(Int64, 4)
    @test typeof(arr2) == Vector{Int64}
    @test eltype(arr2) == Int64
    @test length(arr2) == 4
    @test arr2[1] == 0
    @test arr2[4] == 0

    # Test 3: zeros(Float64, m, n) - multi-dimensional Float64 array
    arr3 = zeros(Float64, 2, 3)
    @test typeof(arr3) == Matrix{Float64}
    @test eltype(arr3) == Float64
    @test size(arr3) == (2, 3)
    @test arr3[1, 1] == 0.0
    @test arr3[2, 3] == 0.0

    # Test 4: zeros(Int64, m, n) - multi-dimensional Int64 array
    arr4 = zeros(Int64, 3, 2)
    @test typeof(arr4) == Matrix{Int64}
    @test eltype(arr4) == Int64
    @test size(arr4) == (3, 2)
    @test arr4[1, 1] == 0
    @test arr4[3, 2] == 0

    # Test 5: ones(Float64, n) - should create Float64 array of ones
    arr5 = ones(Float64, 3)
    @test typeof(arr5) == Vector{Float64}
    @test eltype(arr5) == Float64
    @test length(arr5) == 3
    @test arr5[1] == 1.0
    @test arr5[3] == 1.0

    # Test 6: ones(Int64, n) - should create Int64 array of ones
    arr6 = ones(Int64, 4)
    @test typeof(arr6) == Vector{Int64}
    @test eltype(arr6) == Int64
    @test length(arr6) == 4
    @test arr6[1] == 1
    @test arr6[4] == 1

    # Test 7: ones(Complex{Float64}, n) follows upstream one(T) fill dispatch
    arr7 = ones(Complex{Float64}, 2)
    @test typeof(arr7) == Vector{Complex{Float64}}
    @test eltype(arr7) == Complex{Float64}
    @test length(arr7) == 2
    @test real(arr7[1]) == 1.0
    @test imag(arr7[1]) == 0.0

    # Test 8: zeros(Int, n) - Int is alias for Int64
    arr8 = zeros(Int, 3)
    @test typeof(arr8) == Vector{Int64}
    @test eltype(arr8) == Int64
    @test length(arr8) == 3
    @test arr8[1] == 0
    @test arr8[3] == 0

    # Test 9: typed dispatch no longer goes through Rust-only Float64/Int64/ComplexF64 cases
    arr9 = zeros(Int32, 2)
    @test typeof(arr9) == Vector{Int32}
    @test eltype(arr9) == Int32
    @test arr9[1] == Int32(0)
    @test arr9[2] == Int32(0)

    # Test 10: ones(Int32, n) uses one(::Type{T}) where T<:Number
    arr10 = ones(Int32, 2)
    @test typeof(arr10) == Vector{Int32}
    @test eltype(arr10) == Int32
    @test arr10[1] == Int32(1)
    @test arr10[2] == Int32(1)

    # Test 11: tuple dims use the same dispatch shape as upstream zeros(T, dims::Tuple)
    arr11 = zeros(Float32, (2, 2))
    @test typeof(arr11) == Matrix{Float32}
    @test eltype(arr11) == Float32
    @test size(arr11) == (2, 2)
    @test arr11[2, 2] == Float32(0.0)

    # Test 12: dims computed from size() still dispatch through typed allocation
    # (Issue #4041)
    n = size(arr2)[1]
    arr12 = zeros(Int64, 1, n)
    @test typeof(arr12) == Matrix{Int64}
    @test eltype(arr12) == Int64
    @test size(arr12) == (1, 4)
    @test arr12[1, 4] == 0

    arr13 = ones(Int64, 1, n)
    @test typeof(arr13) == Matrix{Int64}
    @test eltype(arr13) == Int64
    @test size(arr13) == (1, 4)
    @test arr13[1, 4] == 1

    # Test 13: zeros(Complex{Float64}, n) preserves Complex array reflection
    # (Issue #4039)
    arr14 = zeros(Complex{Float64}, 2)
    @test typeof(arr14) == Vector{Complex{Float64}}
    @test eltype(arr14) == Complex{Float64}
    @test typeof(arr14[1]) == Complex{Float64}
    @test real(arr14[1]) == 0.0
    @test imag(arr14[1]) == 0.0

    # Test 14: runtime DataType values still dispatch to typed allocation
    # instead of the dims-only fallback (Issue #4044)
    function make_runtime_ones(T)
        ones(T, 2)
    end

    function make_runtime_zeros(T)
        zeros(T, 2)
    end

    arr15 = make_runtime_ones(Complex{Float64})
    @test typeof(arr15) == Vector{Complex{Float64}}
    @test eltype(arr15) == Complex{Float64}
    @test typeof(arr15[1]) == Complex{Float64}
    @test real(arr15[1]) == 1.0
    @test imag(arr15[1]) == 0.0

    arr16 = make_runtime_zeros(Complex{Float64})
    @test typeof(arr16) == Vector{Complex{Float64}}
    @test eltype(arr16) == Complex{Float64}
    @test typeof(arr16[1]) == Complex{Float64}
    @test real(arr16[1]) == 0.0
    @test imag(arr16[1]) == 0.0

end

true  # Test passed
