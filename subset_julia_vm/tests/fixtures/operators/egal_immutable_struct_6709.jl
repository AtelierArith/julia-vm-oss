# Issue #6709 (and the #6694 StructRef-resolution consolidation):
# `===` (egal) on IMMUTABLE structs stored as heap `StructRef` must compare by
# value, not by heap index. Mutable structs keep reference identity. Also guards
# the sibling StructRef value-ops that route through the same shared resolver:
# tuple `==` (#6685), `hash` (#6693), and `in` membership (#6691).

struct Pt
    x::Int
    y::Int
end

mutable struct MPt
    x::Int
end

checks = Bool[]

# === on immutable user struct: value identity (separately constructed).
push!(checks, Pt(1, 2) === Pt(1, 2))
push!(checks, !(Pt(1, 2) === Pt(1, 3)))

# === on immutable parametric Base struct stored on the heap.
push!(checks, Base.OneTo(3) === Base.OneTo(3))
push!(checks, !(Base.OneTo(3) === Base.OneTo(4)))

# === on a tuple of immutable structs.
push!(checks, (Pt(1, 2),) === (Pt(1, 2),))
push!(checks, !((Pt(1, 2),) === (Pt(1, 3),)))

# === on mutable structs: reference identity (NOT value).
m1 = MPt(1)
m2 = MPt(1)
push!(checks, !(m1 === m2))
push!(checks, m1 === m1)
push!(checks, m2 === MPt(1) ? false : true)  # distinct mutable objects

# === on tuples of mutable structs: per-element reference identity, so a tuple
# of distinct mutable structs is NOT === even when fields match.
push!(checks, (m1,) === (m1,))
push!(checks, !((m1,) === (m2,)))

# Regression: tuple `==` over struct elements (#6685).
push!(checks, (Base.OneTo(3),) == (Base.OneTo(3),))
push!(checks, (Pt(1, 2), Pt(3, 4)) == (Pt(1, 2), Pt(3, 4)))

# Regression: hash consistency for equal immutable structs / tuples (#6693).
push!(checks, hash(Pt(1, 2)) == hash(Pt(1, 2)))
push!(checks, hash((Pt(1, 2),)) == hash((Pt(1, 2),)))
push!(checks, hash(Pt(1, 2)) != hash(Pt(9, 9)))

# Regression: `in` membership over struct / tuple elements (#6691).
push!(checks, Pt(1, 2) in [Pt(0, 0), Pt(1, 2)])
push!(checks, (Pt(1, 2),) in [(Pt(1, 2),)])
push!(checks, !(Pt(5, 5) in [Pt(1, 2), Pt(3, 4)]))

all(checks)
