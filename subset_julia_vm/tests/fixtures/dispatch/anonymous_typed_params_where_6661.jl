# Issue #6661: a method with repeated anonymous typed parameters (each lowered to
# `_`) collapsed those parameters onto a single local slot, so argument binding
# overwrote earlier `_` arguments and `where`-type extraction read every type
# variable from the same slot. `f(::Type{K}, ::Type{V}, n) where {K,V}` then bound
# both K and V to the *second* argument's type (e.g. `Memory{Int64}` for the keys
# of `_new_dict_kv(String, Int64, …)`). Anonymous `_` parameters are positionally
# distinct and never read by name, so each now owns its own slot. Verified against
# upstream Julia 1.12.

using Test

# Two repeated anonymous Type parameters must keep distinct where bindings.
f2(::Type{K}, ::Type{V}, n) where {K,V} = (K, V, n)
# Three repeated anonymous Type parameters.
f3(::Type{A}, ::Type{B}, ::Type{C}) where {A,B,C} = (A, B, C)
# Mixed anonymous Type parameters followed by a named value parameter.
g(::Type{K}, ::Type{V}, name) where {K,V} = (K, V, name)
# Anonymous non-Type parameters preceding a named, where-bound parameter.
h(::Int, ::Int, x::T) where {T} = (x, T)
# The Memory{K}/Memory{V} shape from the Dict storage helper (Issue #6617).
mem(::Type{K}, ::Type{V}, n) where {K,V} =
    (typeof(Memory{K}(undef, n)), typeof(Memory{V}(undef, n)))

ok_f2() = f2(String, Int64, 3) == (String, Int64, 3)
ok_f3() = f3(String, Int64, Bool) == (String, Int64, Bool)
ok_g() = g(Float64, Char, "z") == (Float64, Char, "z")
ok_h() = h(1, 2, 3.0) == (3.0, Float64)
ok_mem() = mem(String, Int64, 1) == (Memory{String}, Memory{Int64})

@testset "repeated anonymous typed parameters keep distinct where bindings (#6661)" begin
    @test ok_f2()
    @test ok_f3()
    @test ok_g()
    @test ok_h()
    @test ok_mem()
end

# Final value gates the in-harness nextest run on correctness, not just no-throw.
ok_f2() && ok_f3() && ok_g() && ok_h() && ok_mem()
