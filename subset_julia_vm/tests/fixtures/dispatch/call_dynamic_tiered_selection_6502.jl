# Issue #6502: Instr::CallDynamic resolves its metadata candidates through
# ordered tiers (all candidates -> user-defined only -> Base `empty`
# allowlist) now driven by the shared selection core. This pins the
# observable outcomes of the tiered path.

# Tier: user-defined methods reached through an Any-typed argument.
struct Box6502
    v::Int
end

unwrap6502(b::Box6502) = b.v
unwrap6502(x) = -1

function via_any_unwrap_6502(x)
    unwrap6502(x)
end

ys = Any[Box6502(7), 1.5]
via_any_unwrap_6502(ys[1]) == 7 || error("expected 7 from Box6502 method")
via_any_unwrap_6502(ys[2]) == -1 || error("expected -1 from fallback method")

# Tier: Base `empty` reached through an Any-typed argument.
function via_any_empty_6502(x)
    empty(x)
end

xs = Any[[1, 2, 3]]
e = via_any_empty_6502(xs[1])
isempty(e) || error("expected empty result")
e isa Vector{Int} || error("expected Vector{Int}, got $(typeof(e))")
true
