using Test

module ModulePrivateTypeObjectReturn8410
abstract type Parent end

struct Hidden <: Parent
    value::Int
end

struct Box{T}
    value::T
end

elem_type(::Box{T}) where {T} = Hidden
elem_type_qualified(::Box) = ModulePrivateTypeObjectReturn8410.Hidden

function local_shadow(::Box)
    Hidden = :shadowed
    Hidden
end
end

box = ModulePrivateTypeObjectReturn8410.Box(1)
returned = ModulePrivateTypeObjectReturn8410.elem_type(box)
qualified = ModulePrivateTypeObjectReturn8410.elem_type_qualified(box)

println(returned)
println(qualified)

@test returned === ModulePrivateTypeObjectReturn8410.Hidden
@test qualified === ModulePrivateTypeObjectReturn8410.Hidden
@test returned <: ModulePrivateTypeObjectReturn8410.Parent
@test ModulePrivateTypeObjectReturn8410.local_shadow(box) == :shadowed

true
