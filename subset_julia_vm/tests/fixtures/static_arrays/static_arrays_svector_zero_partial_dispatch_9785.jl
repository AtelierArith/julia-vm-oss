using StaticArrays
using Test

function svector_zero_dispatch_contract_9785()
    a = SVector{0,Float64}()

    g(x::SVector{0}) = :zero
    eval_rule_like(f, x::SVector{0}, y::SVector{0}) = (f(x), y isa SVector{0})

    direct = g(a) == :zero
    multi_arg = eval_rule_like(x -> 1.7, a, a) == (1.7, true)
    qualified_isa = a isa StaticArrays.SVector{0}

    return (a isa SVector{0}) && qualified_isa && direct && multi_arg
end

@test svector_zero_dispatch_contract_9785()

true
