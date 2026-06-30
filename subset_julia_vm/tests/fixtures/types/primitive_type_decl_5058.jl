using Test

# Issue #5058: user `primitive type Name Bits end` declarations integrate with
# type reflection (isprimitivetype / isbitstype / sizeof / supertype / <: / isa).
# Value construction (MyBits(0x01), reinterpret) is explicitly out of scope.

primitive type MyBits 8 end
primitive type MyU8 <: Unsigned 8 end
primitive type Big512 512 end

@testset "primitive type declarations" begin
    # Bare primitive type (implicit Any supertype)
    @test isprimitivetype(MyBits) == true
    @test isbitstype(MyBits) == true
    @test sizeof(MyBits) == 1
    @test supertype(MyBits) === Any
    @test (MyBits isa Type) == true
    @test MyBits === MyBits

    # Primitive type with an explicit abstract supertype
    @test supertype(MyU8) === Unsigned
    @test (MyU8 <: Unsigned) == true
    # Transitive subtyping through the abstract hierarchy
    @test (MyU8 <: Integer) == true

    # Larger bit width
    @test sizeof(Big512) == 64
end

true
