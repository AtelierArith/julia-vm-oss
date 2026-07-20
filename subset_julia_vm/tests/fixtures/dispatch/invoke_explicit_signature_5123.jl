# Issue #5123: `invoke(f, Tuple{ArgTypes...}, args...)` calls the method matching
# the GIVEN signature type tuple, NOT necessarily the most specific applicable
# method (the upstream `Core.invoke` / `jl_f_invoke` behavior described in
# julia/doc/src/manual/methods.md "Invoking a method on a more general signature").
#
# `f(3)` picks the most specific `f(::Int)`, while `invoke(f, Tuple{Integer}, 3)`
# explicitly selects the more general `f(::Integer)` method. This fixture locks in
# parity with upstream Julia for the representative `invoke` surfaces.

using Test

# Single-argument specificity bypass.
f(::Integer) = :gen
f(::Int) = :spec

# Two-argument signature.
g(x::Number, y::Number) = :number
g(x::Int, y::Int) = :int

# Return value flows into arithmetic.
add(x::Number, y::Number) = x + y + 100
add(x::Int, y::Int) = x + y

# Vararg signature via Tuple{Vararg{T}}.
vf(xs::Int...) = sum(xs)

# Parametric method selected through invoke.
p(x::T) where {T<:Number} = (:param, T)
p(x::Int) = :spec_int

# Three-argument mixed signature.
m(a, b::Integer, c) = (:gen3, a, b, c)
m(a, b::Int, c) = (:spec3, a, b, c)

# A runtime function value must preserve an explicitly declared `Any` rather
# than refining it to the argument's runtime type (Issue #11609).
invoke_any_pick(::Any; offset = 0) = :any
invoke_any_pick(::Int; offset = 0) = :int
invoke_any_value_11609(fn) = invoke(fn, Tuple{Any}, 1)
invoke_any_value_kw_11609(fn) = invoke(fn, Tuple{Any}, 1; offset = 2)

# Prevention matrix: direct/stored callable, static/dynamic signature, and
# positional/keyword lanes must all treat every declared entry as authoritative
# (Issue #11619).
invoke_matrix_pick_11619(::Any; offset = 0) = (:any, offset)
invoke_matrix_pick_11619(::Integer; offset = 0) = (:integer, offset)
invoke_matrix_pick_11619(::Int; offset = 0) = (:int, offset)

invoke_matrix_stored_static_any_pos_11619(fn, x) = invoke(fn, Tuple{Any}, x)
invoke_matrix_stored_static_integer_pos_11619(fn, x) = invoke(fn, Tuple{Integer}, x)
invoke_matrix_stored_static_any_kw_11619(fn, x) = invoke(fn, Tuple{Any}, x; offset = 7)
invoke_matrix_stored_static_integer_kw_11619(fn, x) = invoke(fn, Tuple{Integer}, x; offset = 7)

invoke_matrix_direct_dynamic_pos_11619(sig, x) = invoke(invoke_matrix_pick_11619, sig, x)
invoke_matrix_direct_dynamic_kw_11619(sig, x) = invoke(invoke_matrix_pick_11619, sig, x; offset = 7)
invoke_matrix_stored_dynamic_pos_11619(fn, sig, x) = invoke(fn, sig, x)
invoke_matrix_stored_dynamic_kw_11619(fn, sig, x) = invoke(fn, sig, x; offset = 7)

# A shared-resolver ambiguity is an error, not a no-match that may enter
# constructor/builtin/intrinsic fallbacks (Issue #10461).
ambiguous_value_10461(x::Integer, y) = :left
ambiguous_value_10461(x, y::Integer) = :right
function function_value_is_ambiguous_10461(fn)
    try
        fn(1, 1)
    catch err
        return err isa MethodError && occursin("ambiguous", sprint(showerror, err))
    end
    false
end

@testset "invoke selects the explicitly named signature (Issue #5123)" begin
    # Normal dispatch picks the most specific method ...
    @test f(3) == :spec
    # ... but invoke dispatches to the named (more general) signature.
    @test invoke(f, Tuple{Integer}, 3) == :gen

    @test g(1, 2) == :int
    @test invoke(g, Tuple{Number,Number}, 1, 2) == :number

    @test invoke(add, Tuple{Number,Number}, 2, 3) == 105

    @test invoke(vf, Tuple{Vararg{Int}}, 1, 2, 3) == 6

    @test p(5) == :spec_int
    @test invoke(p, Tuple{Number}, 5) == (:param, Int64)

    @test invoke(m, Tuple{Any,Integer,Any}, 1, 2, 3) == (:gen3, 1, 2, 3)
end

@testset "function-value invoke preserves declared Any (Issue #11609)" begin
    @test invoke_any_value_11609(invoke_any_pick) == :any
    @test invoke_any_value_kw_11609(invoke_any_pick) == :any
end

@testset "declared invoke signature lane matrix (Issue #11619)" begin
    fn = invoke_matrix_pick_11619

    # Direct callable, statically known signature.
    @test invoke(invoke_matrix_pick_11619, Tuple{Any}, 1) == (:any, 0)
    @test invoke(invoke_matrix_pick_11619, Tuple{Integer}, 1) == (:integer, 0)
    @test invoke(invoke_matrix_pick_11619, Tuple{Any}, 1; offset = 7) == (:any, 7)
    @test invoke(invoke_matrix_pick_11619, Tuple{Integer}, 1; offset = 7) == (:integer, 7)

    # Stored callable, statically known signature.
    @test invoke_matrix_stored_static_any_pos_11619(fn, 1) == (:any, 0)
    @test invoke_matrix_stored_static_integer_pos_11619(fn, 1) == (:integer, 0)
    @test invoke_matrix_stored_static_any_kw_11619(fn, 1) == (:any, 7)
    @test invoke_matrix_stored_static_integer_kw_11619(fn, 1) == (:integer, 7)

    # Direct callable, runtime-held signature.
    @test invoke_matrix_direct_dynamic_pos_11619(Tuple{Any}, 1) == (:any, 0)
    @test invoke_matrix_direct_dynamic_pos_11619(Tuple{Integer}, 1) == (:integer, 0)
    @test invoke_matrix_direct_dynamic_kw_11619(Tuple{Any}, 1) == (:any, 7)
    @test invoke_matrix_direct_dynamic_kw_11619(Tuple{Integer}, 1) == (:integer, 7)

    # Stored callable, runtime-held signature.
    @test invoke_matrix_stored_dynamic_pos_11619(fn, Tuple{Any}, 1) == (:any, 0)
    @test invoke_matrix_stored_dynamic_pos_11619(fn, Tuple{Integer}, 1) == (:integer, 0)
    @test invoke_matrix_stored_dynamic_kw_11619(fn, Tuple{Any}, 1) == (:any, 7)
    @test invoke_matrix_stored_dynamic_kw_11619(fn, Tuple{Integer}, 1) == (:integer, 7)
end

@testset "function-value ambiguity is not a dispatch miss (Issue #10461)" begin
    @test function_value_is_ambiguous_10461(ambiguous_value_10461)
end

# `Core.invoke` / `Base.invoke` reach the same dispatch path.
@testset "Base.invoke / Core.invoke parity (Issue #5123)" begin
    @test Base.invoke(f, Tuple{Integer}, 7) == :gen
    @test Core.invoke(f, Tuple{Integer}, 9) == :gen
end

# A function alias and a Tuple-type alias both keep the explicit-signature path.
q(::Integer) = :qgen
q(::Int) = :qspec

@testset "invoke via function alias and signature alias (Issue #5123)" begin
    h = q
    @test invoke(h, Tuple{Integer}, 4) == :qgen

    sig = Tuple{Integer}
    @test invoke(q, sig, 4) == :qgen
end

# Keyword arguments are bound after the explicit-signature method is selected.
kw(x::Number; scale = 1) = x * scale + 1000
kw(x::Int; scale = 1) = x * scale

@testset "invoke preserves keyword arguments (Issue #5123)" begin
    @test kw(3; scale = 2) == 6
    @test invoke(kw, Tuple{Number}, 3; scale = 2) == 1006
end

true
