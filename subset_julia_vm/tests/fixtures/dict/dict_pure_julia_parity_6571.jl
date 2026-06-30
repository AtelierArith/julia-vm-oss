# Issue #6571: parity of the public Dict surface for the legacy literal
# Value::Dict construction path, and dispatch on a Value::Dict reached through an
# Any-typed binding (regression net for the migration; covers the #6584 empty!
# gap). Generic constructors now build struct-backed Dicts in #6618; full
# operation parity across legacy and struct-backed dicts is tracked by #6620.
# Verified against upstream Julia 1.12.
#
# Every op below is routed through a helper whose `d` parameter is Any-typed, so
# each call resolves through runtime method dispatch on a Value::Dict (no
# compile-time fast path). That is exactly the path that bare-vs-parametric
# dispatch gaps and Rust-fallback shadowing break, so it is the safety net the
# #6571 migration is gated on.

using Test

op_len(d) = length(d)
op_has(d, k) = haskey(d, k)
op_get(d, k, dflt) = get(d, k, dflt)
op_idx(d, k) = d[k]

function op_count_keys(d)
    c = 0
    for k in keys(d)
        c += 1
    end
    return c
end

function op_sum_values(d)
    s = 0
    for v in values(d)
        s += v
    end
    return s
end

function op_sum_pairs(d)
    s = 0
    for p in pairs(d)
        s += p.second
    end
    return s
end

# copy must yield an independent dict (fresh DictRef)
function op_copy_then_set(d)
    c = copy(d)
    c["a"] = 100
    return (d["a"], c["a"])
end

# empty! on an Any-typed Value::Dict (Issue #6584) must clear only the copy
function op_copy_then_empty(d)
    c = copy(d)
    empty!(c)
    return (length(c), length(d))
end

# empty! returns the cleared dict
op_empty_returns(d) = length(empty!(d))

# Pure boolean: true iff the full public surface behaves correctly for `d`.
# Used by both the @testset checks and the fixture's final-value gate.
function dict_ok(d)
    return op_len(d) == 2 &&
           op_has(d, "a") && !op_has(d, "z") &&
           op_get(d, "a", -1) == 1 && op_get(d, "z", -1) == -1 &&
           op_idx(d, "a") == 1 && op_idx(d, "b") == 2 &&
           op_count_keys(d) == 2 && op_sum_values(d) == 3 && op_sum_pairs(d) == 3 &&
           op_copy_then_set(d) == (1, 100) &&
           op_copy_then_empty(d) == (0, 2)
end

function merge_ok()
    m = merge(Dict("a" => 1), Dict("b" => 2))
    return op_len(m) == 2 && op_idx(m, "a") == 1 && op_idx(m, "b") == 2
end

empty_ok() = op_copy_then_empty(Dict("a" => 1, "b" => 2)) == (0, 2) &&
             op_empty_returns(Dict("x" => 1)) == 0

function literal_dict_ok()
    return dict_ok(Dict("a" => 1, "b" => 2))
end

function all_ok()
    return literal_dict_ok() && merge_ok() && empty_ok()
end

@testset "Dict surface parity for literal Value::Dict (#6571)" begin
    @test literal_dict_ok()
end

@testset "Dict merge across construction paths (#6571)" begin
    @test merge_ok()
end

@testset "Dict empty! dispatch through Any binding (#6584)" begin
    @test empty_ok()
end

# Final value gates the in-harness nextest run on correctness, not just "did not
# throw": a bare `true` would let an internal @test failure pass silently
# (fixture-harness convention; cf. PR #5946/#5935). A wrong value flips this to
# `false` and fails the chunk.
all_ok()
