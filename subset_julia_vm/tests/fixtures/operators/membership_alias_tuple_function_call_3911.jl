@assert in(1, (1, 2))
@assert ∈(1, (1, 2))
@assert ∉(3, (1, 2))
@assert ∋((1, 2), 2)
@assert ∌((1, 2), 3)

@assert Base.:∈(1, (1, 2))
@assert Base.:(∈)(1, (1, 2))
@assert Base.:∉(3, (1, 2))
@assert Base.:∋((1, 2), 2)
@assert Base.:∌((1, 2), 3)

true
