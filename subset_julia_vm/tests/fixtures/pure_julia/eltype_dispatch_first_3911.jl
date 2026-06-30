using Test

struct EltypeDispatchBox3911
    n::Int64
end

Base.eltype(::EltypeDispatchBox3911) = Float32
Base.eltype(::Type{EltypeDispatchBox3911}) = Int16

box = EltypeDispatchBox3911(7)

@test Base.eltype(box) === Float32
@test eltype(box) === Float32
@test Base.eltype(EltypeDispatchBox3911) === Int16

values = [1, 2, 3]
@test eltype(values) === Int64

true
