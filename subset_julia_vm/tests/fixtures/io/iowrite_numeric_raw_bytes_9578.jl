using Test

function assert_write_bytes_9578(value, expected)
    io = IOBuffer()
    n = write(io, value)
    bytes = String(take!(io))

    @test n == length(expected)
    @test ncodeunits(bytes) == length(expected)
    for i in 1:length(expected)
        @test codeunit(bytes, i) == expected[i]
    end
end

@testset "write(io, numeric) emits raw bytes" begin
    assert_write_bytes_9578(Int8(42), UInt8[0x2a])
    assert_write_bytes_9578(UInt8(0xff), UInt8[0xff])
    assert_write_bytes_9578(Int16(-2), UInt8[0xfe, 0xff])
    assert_write_bytes_9578(UInt16(0x0201), UInt8[0x01, 0x02])
    assert_write_bytes_9578(Int32(0x01020304), UInt8[0x04, 0x03, 0x02, 0x01])
    assert_write_bytes_9578(UInt64(0x0102030405060708), UInt8[0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01])
    assert_write_bytes_9578(UInt128(0x0102), UInt8[0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
    assert_write_bytes_9578(Float32(1.5), UInt8[0x00, 0x00, 0xc0, 0x3f])
    assert_write_bytes_9578(Float64(1.5), UInt8[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf8, 0x3f])
    assert_write_bytes_9578(true, UInt8[0x01])
    assert_write_bytes_9578(false, UInt8[0x00])
    assert_write_bytes_9578(Char(0x41), UInt8[0x41])
end

true
