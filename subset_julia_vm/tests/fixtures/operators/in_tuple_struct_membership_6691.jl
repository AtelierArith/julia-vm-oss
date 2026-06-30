# Issue #6691: `in` / `∈` over a collection of tuples (or bare structs) must fold
# `==` over the elements. The native `In` builtin's ad-hoc scalar comparison had
# no tuple / named-tuple / struct arm, so any tuple element fell through to
# `false` — even a primitive `(1, 2) in [(1, 2)]`. Heap-stored struct elements
# (e.g. `OneTo`) additionally need the #6685 struct-ref resolution to compare by
# value. All assertions match upstream Julia 1.12.

checks = Bool[]

# --- primitive tuples in an array ----------------------------------------
push!(checks, (1, 2) in [(1, 2), (3, 4)])
push!(checks, !((9, 9) in [(1, 2), (3, 4)]))
push!(checks, (1, 2) ∈ [(1, 2), (3, 4)])

# --- primitive tuple in a tuple-of-tuples --------------------------------
push!(checks, (3, 4) in ((1, 2), (3, 4)))
push!(checks, !((5, 6) in ((1, 2), (3, 4))))

# --- struct (OneTo) tuple elements compare by value ----------------------
push!(checks, (Base.OneTo(3),) in [(Base.OneTo(3),)])
push!(checks, !((Base.OneTo(3),) in [(Base.OneTo(4),)]))
push!(checks, (Base.OneTo(2), Base.OneTo(2)) in [(Base.OneTo(2), Base.OneTo(2))])

# --- bare struct elements ------------------------------------------------
push!(checks, Base.OneTo(3) in [Base.OneTo(3), Base.OneTo(5)])
push!(checks, !(Base.OneTo(9) in [Base.OneTo(3), Base.OneTo(5)]))

# --- named tuples --------------------------------------------------------
push!(checks, (a = 1, b = 2) in [(a = 1, b = 2)])
push!(checks, !((a = 1, b = 2) in [(a = 1, b = 3)]))

# --- UnitRange / Complex tuple elements ----------------------------------
push!(checks, (1:3,) in [(1:3,)])
push!(checks, (1 + 2im,) in [(1 + 2im,)])

# --- existing scalar membership still works ------------------------------
push!(checks, 3 in [1, 2, 3])
push!(checks, "x" in ["a", "x"])
push!(checks, 1:3 in [1:3, 4:6])
push!(checks, Int in [Float64, Int])

all(checks)
