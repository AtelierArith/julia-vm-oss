# Qualified access to Base const type aliases (Issue #10579)
#
# Upstream allows `Base.Bottom` / `Base.BitSigned` etc. even though the
# aliases are not exported. `Bottom` stays qualified-only (a flat alias
# would leak the bare name into Main — Issues #10304/#10578).

using Test

@testset "Base const type aliases" begin
    @test Base.Bottom === Union{}
    @test 1 isa Base.BitSigned
    @test 0x01 isa Base.BitUnsigned
    @test 1 isa Base.BitInteger
    @test !(1.0 isa Base.BitInteger)
    @test Base.BitSigned isa Union
end

true
