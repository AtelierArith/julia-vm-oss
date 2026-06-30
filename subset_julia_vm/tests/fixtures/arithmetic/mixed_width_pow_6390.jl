using Test

# Issue #6390: mixed-width primitive integer powers must stay on the VM
# integer-power path instead of dispatching back to generic ^(::Number, ::Integer).

pow_dynamic_6390(x, y) = x ^ y

@testset "mixed-width integer powers preserve base type" begin
    @test typeof(Int8(2) ^ Int16(3)) === Int8
    @test (Int8(2) ^ Int16(3)) === Int8(8)
    @test typeof(Int8(2) ^ Int32(3)) === Int8
    @test (Int8(2) ^ Int32(3)) === Int8(8)

    @test typeof(Int16(2) ^ Int8(3)) === Int16
    @test (Int16(2) ^ Int8(3)) === Int16(8)
    @test typeof(Int64(2) ^ Int8(3)) === Int64
    @test (Int64(2) ^ Int8(3)) === Int64(8)

    @test typeof(UInt8(2) ^ UInt16(3)) === UInt8
    @test (UInt8(2) ^ UInt16(3)) === UInt8(8)
    @test typeof(UInt8(2) ^ Int16(3)) === UInt8
    @test (UInt8(2) ^ Int16(3)) === UInt8(8)
    @test typeof(Int8(2) ^ UInt16(3)) === Int8
    @test (Int8(2) ^ UInt16(3)) === Int8(8)
    @test typeof(UInt16(2) ^ UInt8(3)) === UInt16
    @test (UInt16(2) ^ UInt8(3)) === UInt16(8)
end

@testset "dynamic mixed-width integer powers" begin
    @test pow_dynamic_6390(Int8(2), Int16(3)) === Int8(8)
    @test typeof(pow_dynamic_6390(Int8(2), Int16(3))) === Int8
    @test pow_dynamic_6390(UInt8(2), Int16(3)) === UInt8(8)
    @test typeof(pow_dynamic_6390(UInt8(2), Int16(3))) === UInt8
end

@testset "Bool and negative integer exponent parity" begin
    @test typeof(true ^ Int8(3)) === Bool
    @test (true ^ Int8(3)) === true
    @test (false ^ Int8(0)) === true
    @test (false ^ UInt64(0)) === true
    @test (false ^ UInt64(4294967296)) === false
    @test (Int8(2) ^ false) === Int8(1)
    @test (Int8(2) ^ true) === Int8(2)

    @test_throws DomainError Int8(2) ^ Int16(-1)
    @test_throws DomainError UInt8(2) ^ Int16(-1)
    @test_throws DomainError Int64(2) ^ Int64(-1)
    @test_throws DomainError false ^ Int8(-1)
end

true
