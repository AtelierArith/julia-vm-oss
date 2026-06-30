using Test

struct HasfieldDispatchBox3911
    n::Int64
end

Base.hasfield(::HasfieldDispatchBox3911, s::Symbol) = s === :sentinel

box = HasfieldDispatchBox3911(7)

@test Base.hasfield(box, :sentinel)
@test !Base.hasfield(box, :other)
@test Base.hasfield(HasfieldDispatchBox3911, :n)

true
