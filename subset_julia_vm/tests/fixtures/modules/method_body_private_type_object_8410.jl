using Test

module MethodBodyPrivateTypeObject8410
abstract type Parent end

mutable struct Hidden <: Parent
    x::Int
end

struct Box{T}
    x::T
end

elem_type(::Box{T}) where {T} = Hidden
make_hidden(x) = elem_type(Box(x))(x)
end

@testset "module method body resolves private type object (Issue #8410)" begin
    b = MethodBodyPrivateTypeObject8410.Box(1)

    @test MethodBodyPrivateTypeObject8410.elem_type(b) === MethodBodyPrivateTypeObject8410.Hidden
    @test typeof(MethodBodyPrivateTypeObject8410.elem_type(b)) === DataType
    @test MethodBodyPrivateTypeObject8410.elem_type(b) <: MethodBodyPrivateTypeObject8410.Parent

    value = MethodBodyPrivateTypeObject8410.make_hidden(7)
    @test value isa MethodBodyPrivateTypeObject8410.Hidden
    @test value.x == 7
end

true
