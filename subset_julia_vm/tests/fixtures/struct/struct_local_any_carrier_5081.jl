using Test

mutable struct StructCarrierBox5081
    value::Int
end

struct_global_roundtrip_5081 = StructCarrierBox5081(3)
struct_global_reassign_5081 = StructCarrierBox5081(4)

@testset "Struct fallback locals use locals_any carrier" begin
    global struct_global_roundtrip_5081
    global struct_global_reassign_5081

    @test struct_global_roundtrip_5081.value == 3
    struct_global_roundtrip_5081.value = 7
    @test struct_global_roundtrip_5081.value == 7

    struct_global_roundtrip_5081 = StructCarrierBox5081(9)
    @test struct_global_roundtrip_5081.value == 9

    struct_global_reassign_5081 = 42
    @test struct_global_reassign_5081 == 42
end

struct_global_roundtrip_5081.value == 9 && struct_global_reassign_5081 == 42
