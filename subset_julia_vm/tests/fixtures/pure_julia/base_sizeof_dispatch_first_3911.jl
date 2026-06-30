using Test

struct SizeofDispatchBox3911
    n::Int64
end

Base.sizeof(b::SizeofDispatchBox3911) = b.n + 100

box = SizeofDispatchBox3911(7)

@test sizeof(box) == 107
@test Base.sizeof(box) == 107
@test sizeof(Int64) == 8
@test Base.sizeof(Int64) == 8

true
