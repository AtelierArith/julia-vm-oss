using Test
using InteractiveUtils

# Regression for Issue #3908: builtins_types.rs routes the `subtypes(...)`
# result through a shared Memory-first `any_vector` helper. Exercise both the
# empty-result branch (`subtypes(::Type)` on a concrete leaf) and the
# populated-result branch (`subtypes(Signed)`) so the routed construction is
# observable end-to-end without depending on the sjulia-specific scalar
# fallback path.

function find_subtype_3908(types, name)
    for i in 1:length(types)
        if string(types[i]) == name
            return true
        end
    end
    return false
end

@testset "subtypes any_vector helper (Issue #3908)" begin
    # Empty-result branch: concrete leaf has no direct subtypes, exercising
    # the routed `any_vector(Vec::new())` construction.
    empty_result = subtypes(Int64)
    @test empty_result isa AbstractVector
    @test length(empty_result) == 0
    @test isempty(empty_result)
    @test size(empty_result) == (0,)

    # Populated-result branch: routed construction preserves shape and
    # allows the elements to roundtrip through `string`.
    signed_children = subtypes(Signed)
    @test signed_children isa AbstractVector
    @test length(signed_children) >= 1
    @test size(signed_children) == (length(signed_children),)
    @test all(t -> t isa Type, signed_children)
    @test find_subtype_3908(signed_children, "Int64")
    @test find_subtype_3908(signed_children, "Int32")
end

true
