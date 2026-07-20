# Issue #10004: take!(IOBuffer) returns raw bytes, not a String.

using Test

@testset "take!(IOBuffer) returns Vector{UInt8}" begin
    io = IOBuffer()
    @test write(io, "ab") == 2
    bytes = take!(io)
    @test typeof(bytes) == Vector{UInt8}
    @test bytes == UInt8[0x61, 0x62]
    @test String(bytes) == "ab"
    @test take!(io) == UInt8[]

    raw = IOBuffer()
    @test write(raw, UInt8(0xff)) == 1
    raw_bytes = take!(raw)
    @test typeof(raw_bytes) == Vector{UInt8}
    @test raw_bytes == UInt8[0xff]
end

true
