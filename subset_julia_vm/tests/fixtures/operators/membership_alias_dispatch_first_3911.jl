struct AliasNeedle3911
    value::Int64
end

struct AliasBox3911
    value::Int64
end

function Base.in(x::AliasNeedle3911, b::AliasBox3911)
    return x.value == b.value
end

needle = AliasNeedle3911(3)
box = AliasBox3911(3)
other = AliasNeedle3911(4)

@assert needle ∈ box
@assert !(other ∈ box)
@assert other ∉ box
@assert box ∋ needle
@assert box ∌ other

@assert 1 ∈ (1, 2)
@assert 3 ∉ (1, 2)
@assert (1, 2) ∋ 2
@assert (1, 2) ∌ 3

true
