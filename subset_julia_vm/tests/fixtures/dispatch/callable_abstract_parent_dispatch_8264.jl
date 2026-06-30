using Test

abstract type AbstractCallableParent8264 end

struct ConcreteCallableParent8264{T} <: AbstractCallableParent8264
    tag::T
end

struct CallableArg8264{T}
    value::T
end

function (parent::AbstractCallableParent8264)(x::CallableArg8264{T}, y::CallableArg8264{T}) where T
    return CallableArg8264{T}(parent.tag + x.value - y.value)
end

parent = ConcreteCallableParent8264{Int}(10)
result = parent(CallableArg8264{Int}(7), CallableArg8264{Int}(3))

@test result.value == 14
@test typeof(result) === CallableArg8264{Int}

true
