# Issue #6740 (precursor): reinterpret between Float16 and UInt16/Int16 was
# unimplemented (the Rust handler errored "size mismatch"). It is needed by the
# pure-Julia float-decomposition functions for Float16. Values match upstream
# julia 1.12.

using Test

@testset "reinterpret Float16 <-> UInt16/Int16 (Issue #6740)" begin
    @test reinterpret(UInt16, Float16(1.0)) === UInt16(0x3c00)
    @test reinterpret(Float16, UInt16(0x3c00)) === Float16(1.0)
    @test reinterpret(UInt16, Float16(2.0)) === UInt16(0x4000)
    @test reinterpret(UInt16, Float16(0.0)) === UInt16(0x0000)
    @test reinterpret(UInt16, Float16(-1.0)) === UInt16(0xbc00)
    @test reinterpret(Int16, Float16(1.0)) === Int16(0x3c00)
    # round-trips
    @test reinterpret(Float16, reinterpret(UInt16, Float16(3.5))) === Float16(3.5)
    @test reinterpret(UInt16, reinterpret(Float16, UInt16(0x1234))) === UInt16(0x1234)
end

true
