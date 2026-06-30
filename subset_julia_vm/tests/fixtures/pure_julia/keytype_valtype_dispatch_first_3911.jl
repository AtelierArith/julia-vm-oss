using Test

struct KeyValDispatchBox3911
    n::Int64
end

Base.keytype(b::KeyValDispatchBox3911) = Int64
Base.valtype(b::KeyValDispatchBox3911) = String

box = KeyValDispatchBox3911(7)

@test Base.keytype(box) === Int64
@test Base.valtype(box) === String

d = Dict()
@test keytype(d) === Any
@test valtype(d) === Any

true
