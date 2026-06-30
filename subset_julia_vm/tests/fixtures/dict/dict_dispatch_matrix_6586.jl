# Issue #6586: reconcile Value::Dict vs Dict{K,V} StructRef dispatch so the
# retained Rust Dict fallback never shadows a public Base method.
#
# This routes every public Dict op through an Any-typed function parameter, so
# each resolves via runtime method dispatch on a Value::Dict (no compile-time
# fast path) — the path that bare-vs-parametric gaps break (cf. #6584 empty!,
# #6585 get!). The complementary "user-defined methods win over the Rust
# fallback" property is covered by dict_setindex_struct_dispatch.jl,
# dict_delete_struct_dispatch.jl, and dict_user_method_wins_6586.jl. All
# expected values verified against upstream Julia 1.12.

using Test

# ----- public Dict surface through Any-typed dispatch -----
g_idx(d) = d["a"]
g_set(d) = (d["q"] = 7; d["q"])
g_get(d) = get(d, "a", -1)
g_get_miss(d) = get(d, "zz", -1)
g_getbang(d) = get!(d, "z", 99)
g_getkey(d) = getkey(d, "a", "none")
g_haskey(d) = haskey(d, "a")
g_len(d) = length(d)
g_isempty(d) = isempty(d)
g_delete(d) = (delete!(d, "a"); length(d))
g_empty(d) = (empty!(d); length(d))
g_pop2(d) = pop!(d, "a")
g_pop3(d) = pop!(d, "zz", -1)
g_copy(d) = (c = copy(d); c["a"] = 100; (d["a"], c["a"]))
g_merge(a, b) = (m = merge(a, b); length(m))
g_mergebang(a, b) = (merge!(a, b); length(a))
function g_keys(d)
    c = 0
    for k in keys(d); c += 1; end
    c
end
function g_values(d)
    s = 0
    for v in values(d); s += v; end
    s
end
function g_pairs(d)
    s = 0
    for p in pairs(d); s += p.second; end
    s
end
function g_iter(d)
    s = 0
    for (k, v) in d; s += v; end
    s
end
g_first(d) = first(d).second

mk() = Dict("a" => 1, "b" => 2)

function surface_ok()
    return g_idx(mk()) == 1 &&
           g_set(mk()) == 7 &&
           g_get(mk()) == 1 &&
           g_get_miss(mk()) == -1 &&
           g_getbang(mk()) == 99 &&
           g_getkey(mk()) == "a" &&
           g_haskey(mk()) == true &&
           g_len(mk()) == 2 &&
           g_isempty(mk()) == false &&
           g_delete(mk()) == 1 &&
           g_empty(mk()) == 0 &&
           g_pop2(mk()) == 1 &&
           g_pop3(mk()) == -1 &&
           g_copy(mk()) == (1, 100) &&
           g_merge(mk(), Dict("c" => 3)) == 3 &&
           g_mergebang(mk(), Dict("c" => 3)) == 3 &&
           g_keys(mk()) == 2 &&
           g_values(mk()) == 3 &&
           g_pairs(mk()) == 3 &&
           g_iter(mk()) == 3 &&
           (g_first(mk()) in (1, 2))
end

all_ok() = surface_ok()

@testset "Dict dispatch matrix: Value::Dict surface via Any (#6586)" begin
    @test surface_ok()
end

all_ok()
