# Issue #6586: user-defined methods extending Base Dict-family functions on a
# custom type must win over the retained Rust `Value::Dict` fallback (no
# shadowing), while the Base path still serves real Dicts. Dispatch is observed
# through a mutable call log so the check is independent of each method's return
# type. Methods are added with `Base.` so they extend (not replace) the generic
# functions. Verified against upstream Julia 1.12.
#
# Complements dict_setindex_struct_dispatch.jl / dict_delete_struct_dispatch.jl
# (which cover setindex!/delete! struct dispatch directly).

using Test

mutable struct Spy
    log::Vector{String}
end
Base.get!(x::Spy, k, v) = (push!(x.log, "get!"); v)
Base.empty!(x::Spy) = (push!(x.log, "empty!"); x)
Base.delete!(x::Spy, k) = (push!(x.log, "delete!"); x)
Base.pop!(x::Spy, k) = (push!(x.log, "pop!"); x)
Base.merge!(a::Spy, b) = (push!(a.log, "merge!"); a)

# Drive each op through an Any-typed parameter so resolution is at runtime; the
# return value is discarded — we only assert the user method ran.
run_empty(x) = (empty!(x); nothing)
run_delete(x) = (delete!(x, "k"); nothing)
run_pop(x) = (pop!(x, "k"); nothing)
run_merge(x) = (merge!(x, x); nothing)
run_getbang(x) = (get!(x, "k", 1); nothing)

function user_methods_win()
    s = Spy(String[])
    run_empty(s)
    run_delete(s)
    run_pop(s)
    run_merge(s)
    run_getbang(s)
    return s.log == ["empty!", "delete!", "pop!", "merge!", "get!"]
end

# The Base Dict path must remain intact for real dicts.
function base_dict_intact()
    d = Dict("a" => 1)
    get!(d, "z", 9)
    ok = get!(d, "z", 0) == 9 && haskey(d, "z") && length(d) == 2
    empty!(d)
    return ok && length(d) == 0
end

all_ok() = user_methods_win() && base_dict_intact()

@testset "user methods win over Rust Dict fallback (#6586)" begin
    @test user_methods_win()
end

@testset "Base Dict path intact alongside user methods (#6586)" begin
    @test base_dict_intact()
end

all_ok()
