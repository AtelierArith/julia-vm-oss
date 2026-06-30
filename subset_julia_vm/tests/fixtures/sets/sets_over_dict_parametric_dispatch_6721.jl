# Issue #6721: Set{T} re-implemented as a pure-Julia Dict{T,Nothing} wrapper.
#
# The headline regression: a user-defined parametric `Set{T}` method must be
# able to extract the element type, exactly like Dict/Array. Before this change
# the native `Value::Set` carrier did not participate in `Set{T}` struct
# method-table dispatch, so `ft(Set([1,2,3]))` threw a MethodError instead of
# returning `Int64`.

# --- headline divergence (RED -> GREEN) -----------------------------------
ft(x::Set{T}) where {T} = T
@assert ft(Set([1, 2, 3])) == Int64

# --- typeof / isa / eltype parity -----------------------------------------
@assert typeof(Set([1, 2, 3])) == Set{Int64}
@assert Set([1, 2, 3]) isa Set{Int64}
@assert eltype(Set([1, 2, 3])) == Int64
@assert eltype(Set{Float64}()) == Float64

# --- core ops: push! / in / length / delete! / empty! / isempty -----------
s = Set{Int64}()
push!(s, 1)
push!(s, 2)
push!(s, 2)
@assert length(s) == 2
@assert 1 in s
@assert !(3 in s)
@assert 2 ∈ s
@assert 5 ∉ s
delete!(s, 1)
@assert !(1 in s)
@assert length(s) == 1
empty!(s)
@assert length(s) == 0
@assert isempty(s)

# --- Set() default and Set(itr) -------------------------------------------
e = Set()
@assert eltype(e) == Any
@assert length(Set([1, 1, 2, 3, 3])) == 3

# --- set algebra ----------------------------------------------------------
@assert issetequal(union(Set([1, 2, 3]), Set([2, 3, 4])), Set([1, 2, 3, 4]))
@assert issetequal(intersect(Set([1, 2, 3]), Set([2, 3, 4])), Set([2, 3]))
@assert issetequal(setdiff(Set([1, 2, 3]), Set([2])), Set([1, 3]))
@assert issubset(Set([1, 2]), Set([1, 2, 3]))
@assert isdisjoint(Set([1, 2]), Set([3, 4]))

# --- Set of tuples membership (Issue #6693 follow-up) ---------------------
st = Set([(1, 2), (3, 4)])
@assert (1, 2) in st
@assert !((5, 6) in st)
@assert length(st) == 2

# --- Set of strings -------------------------------------------------------
ss = Set(["a", "b", "a"])
@assert length(ss) == 2
@assert "a" in ss
@assert eltype(ss) == String

# --- copy is independent (mutate the original, copy is unaffected) --------
orig = Set([1, 2, 3])
dup = copy(orig)
push!(orig, 4)
@assert length(orig) == 4
@assert length(dup) == 3
@assert !(4 in dup)

# --- iteration ------------------------------------------------------------
total = 0
for x in Set([1, 2, 3, 4])
    global total += x
end
@assert total == 10

true
