# Issue #5083: array element types beyond Int64/Float64 must propagate through
# inference so that `a[i]` yields the concrete element type instead of `Any`.
# This previously collapsed to `Top` for Int8/16/32, UInt*, Float32, Int128,
# UInt128, Symbol, and Complex element arrays.
using Test

# Functions must be defined OUTSIDE the @testset block per project guidelines.
function index_type(arr)
    return typeof(arr[1])
end

function sum_scan(arr)
    # A numeric scan whose accumulator type depends on the element type being
    # preserved through inference.
    s = zero(eltype(arr))
    for i in eachindex(arr)
        s += arr[i]
    end
    return s
end

@testset "Issue #5083 array element type propagation" begin
    @test index_type(Int8[1, 2, 3]) === Int8
    @test index_type(Int16[1, 2, 3]) === Int16
    @test index_type(Int32[1, 2, 3]) === Int32
    @test index_type(UInt8[1, 2, 3]) === UInt8
    @test index_type(UInt16[1, 2, 3]) === UInt16
    @test index_type(UInt32[1, 2, 3]) === UInt32
    @test index_type(UInt64[1, 2, 3]) === UInt64
    @test index_type(Float32[1.0, 2.0, 3.0]) === Float32
    @test index_type([:a, :b, :c]) === Symbol

    # Numeric scans preserve the element type in the accumulator.
    @test sum_scan(Int8[1, 2, 3]) === Int8(6)
    @test sum_scan(Int32[10, 20, 30]) === Int32(60)
    @test sum_scan(UInt8[1, 2, 3]) === UInt8(6)
    @test sum_scan(Float32[1.0, 2.0, 3.0]) === 6.0f0
end

true
