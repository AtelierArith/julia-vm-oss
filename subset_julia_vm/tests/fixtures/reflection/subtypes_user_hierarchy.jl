# Issue #3768: subtypes(T) must include user-defined direct children.
# `subtypes` lives in InteractiveUtils upstream (sjulia also exposes it there);
# import it so the fixture runs under upstream julia too (Issue #10237).

using Test
using InteractiveUtils

abstract type ReflectionRoot3768 end
abstract type ReflectionMid3768 <: ReflectionRoot3768 end

struct ReflectionLeaf3768 <: ReflectionMid3768
    x::Int64
end

mutable struct ReflectionMutableLeaf3768 <: ReflectionRoot3768
    y::Int64
end

struct ReflectionPlain3768
    z::Int64
end

function reflection_has_type_3768(types, name)
    for i in 1:length(types)
        if string(types[i]) == name
            return true
        end
    end
    false
end

@testset "subtypes user-defined direct hierarchy" begin
    root_children = subtypes(ReflectionRoot3768)
    @test reflection_has_type_3768(root_children, "ReflectionMid3768")
    @test reflection_has_type_3768(root_children, "ReflectionMutableLeaf3768")
    @test !reflection_has_type_3768(root_children, "ReflectionLeaf3768")

    mid_children = subtypes(ReflectionMid3768)
    @test length(mid_children) == 1
    @test reflection_has_type_3768(mid_children, "ReflectionLeaf3768")

    any_children = subtypes(Any)
    @test reflection_has_type_3768(any_children, "ReflectionRoot3768")
    @test reflection_has_type_3768(any_children, "ReflectionPlain3768")
end

true
