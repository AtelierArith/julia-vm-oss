# A Base method on Base.Partition must not accept a same-named module struct.

using Test

module StructOrigin10295
export KeySet, Partition, Rational

struct Partition
    n::Int
    part::Vector
end

struct Rational{T}
    value::T
end

struct KeySet end

struct AbstractDisplay <: Base.AbstractDisplay end

end

using .StructOrigin10295

p10295 = StructOrigin10295.Partition(2, [10, 20])
err10295 = try
    length(p10295)
    nothing
catch err
    err
end

@test err10295 isa MethodError

length10295 = length
err_function10295 = try
    length10295(p10295)
    nothing
catch err
    err
end

@test err_function10295 isa MethodError

r10295 = StructOrigin10295.Rational{Int}(3)
numerator10295 = numerator
err_parametric10295 = try
    numerator10295(r10295)
    nothing
catch err
    err
end

@test err_parametric10295 isa MethodError

io10295 = IOBuffer()
show(io10295, StructOrigin10295.KeySet())
@test String(take!(io10295)) == "KeySet()"

@test collect(Iterators.partition([1, 2, 3], 2)) == [[1, 2], [3]]

display10295 = StructOrigin10295.AbstractDisplay()
@test begin
    pushdisplay(display10295)
    true
end

true
