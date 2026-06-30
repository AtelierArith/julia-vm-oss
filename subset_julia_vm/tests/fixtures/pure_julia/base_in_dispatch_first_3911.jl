using Test

struct InDispatchBox3911
    n::Int64
end

Base.in(x::Int64, b::InDispatchBox3911) = x == b.n

box = InDispatchBox3911(7)

@test Base.in(7, box) == true
@test Base.in(3, box) == false

@test Base.in(2, (1, 2, 3)) == true
@test Base.in(5, (1, 2, 3)) == false

true
