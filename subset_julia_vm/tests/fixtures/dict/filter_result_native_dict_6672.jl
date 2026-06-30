# Issue #6672: `filter(pred, d::Dict)` widened to an `Any`-typed binding because the
# call-site return-type inference only preserved the container type for arrays and
# fell through to a fragile interprocedural estimate for dicts, which varied with
# cache/dispatch order. When the result inferred as `Any`, a following
# `empty!(filtered)` was routed through the legacy `DictEmpty` boundary instruction
# instead of native struct-backed dispatch (the Issue #6621 invariant), so the
# bytecode shape depended on inference state. `filter` now propagates the receiver's
# `Dict{K,V}` / `Set{T}` type at the call site, matching upstream (filtering only
# drops entries, preserving the container type). Verified against upstream Julia 1.12.

using Test

# `filtered` is the result of `filter` reached through an inferred binding; the
# following dict ops must dispatch to the native struct-backed dict, not widen to
# `Any` and demote to a legacy boundary.
function filter_then_empty()
    d = Dict("a" => 1, "b" => 2, "c" => 3)
    filtered = filter(p -> p.second > 1, d)   # {"b"=>2, "c"=>3}
    n_before = length(filtered)
    has_b = haskey(filtered, "b")
    has_a = haskey(filtered, "a")
    emptied = empty!(filtered)                # `filtered` now empty; returns it
    return (n_before, has_b, has_a, length(filtered), isempty(emptied), length(d))
end

function filter_result_is_mutable_dict()
    d = Dict(1 => 10, 2 => 20, 3 => 30)
    filtered = filter(p -> p.first != 2, d)   # {1=>10, 3=>30}
    filtered[4] = 40                          # setindex! on the filtered dict
    return length(filtered) == 3 && filtered[4] == 40 && !haskey(d, 4)
end

ok_filter_empty() = filter_then_empty() == (2, true, false, 0, true, 3)
ok_filter_mutable() = filter_result_is_mutable_dict()

@testset "filter(::Dict) result stays a native dict for empty!/setindex! (#6672)" begin
    @test ok_filter_empty()
    @test ok_filter_mutable()
end

# Final value gates the in-harness nextest run on correctness, not just no-throw.
ok_filter_empty() && ok_filter_mutable()
