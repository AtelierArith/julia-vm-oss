using Test
using InteractiveUtils

function reflection_has_type_3837(types, name)
    for i in 1:length(types)
        if string(types[i]) == name
            return true
        end
    end
    false
end

@testset "subtypes builtin core hierarchy" begin
    signed_children = subtypes(Signed)
    @test reflection_has_type_3837(signed_children, "Int8")
    @test reflection_has_type_3837(signed_children, "Int16")
    @test reflection_has_type_3837(signed_children, "Int32")
    @test reflection_has_type_3837(signed_children, "Int64")
    @test reflection_has_type_3837(signed_children, "Int128")
    @test reflection_has_type_3837(signed_children, "BigInt")

    float_children = subtypes(AbstractFloat)
    @test reflection_has_type_3837(float_children, "Float16")
    @test reflection_has_type_3837(float_children, "Float32")
    @test reflection_has_type_3837(float_children, "Float64")
    @test reflection_has_type_3837(float_children, "BigFloat")

    type_children = subtypes(Type)
    @test reflection_has_type_3837(type_children, "DataType")
end

true
