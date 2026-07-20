# Issue #9301: `collect` over a generator whose element type is Float16 must
# narrow the container element type to Float16, not widen it to Any. Every other
# primitive numeric producer (Float32/Float64/Int/Bool) already narrowed; Float16
# was the missing arm in the generator->collect element-type path.
#
# Fix-forward (PR #9368): the same Float16 arm was threaded through every sibling
# element-type map that already handled Float32/Float64, so the empty typed
# literal `Float16[]`, the dot-broadcast constructor `Float16.(xs)`, the range
# comprehension and `map` over a bound Float16 array all report `Vector{Float16}`
# too. Every container-eltype check below is folded into the final asserted
# value so a regression in ANY of these sibling maps flips the result to `false`.

f16() = Float16(1)
f32() = Float32(1)
f64() = Float64(1)
fint() = Int(1)
fbool() = Bool(1)

# Container type parity across producers (the issue's table).
println(typeof(collect(f16() for _ in 1:3)))
println(typeof(collect(f32() for _ in 1:3)))
println(typeof(collect(f64() for _ in 1:3)))
println(typeof(collect(fint() for _ in 1:3)))
println(typeof(collect(fbool() for _ in 1:3)))

# eltype keyed on the container type must agree with typeof.
println(eltype(collect(f16() for _ in 1:3)))
println(eltype(collect(f32() for _ in 1:3)))

# The individual elements were already correct Float16 values; only the
# container widened. Verify both element value type and container type together.
xs = collect(f16() for _ in 1:3)
println(xs)
println(typeof(xs[1]))

# Comprehension form of the same generator narrows identically.
println(typeof([Float16(i) for i in 1:3]))

# Constructor / typed-literal forms that share the same Float16 element-type
# mapping now report Vector{Float16} too.
println(typeof(Vector{Float16}(undef, 3)))
println(typeof(Float16[Float16(1), Float16(2)]))

# Typed literal `Float16[...]` converts Int elements to Float16 (setindex! ->
# convert(Float16, x)) and reads them back as Float16.
ys = Float16[1, 2, 3]
println(typeof(ys), " ", ys[1], " ", typeof(ys[1]))

# Vector{Float16}(undef, n) followed by assignment stores/reads Float16 values.
zs = Vector{Float16}(undef, 2)
zs[1] = Float16(5)
zs[2] = Float16(6)
println(zs, " ", typeof(zs[1]))

# Dispatch keyed on the concrete container type resolves to the Float16 method.
pick(::Vector{Float16}) = "f16vec"
pick(::Vector{Int}) = "intvec"
println(pick(ys))

# --- Sibling element-type producers threaded by the PR #9368 fix-forward. ---
# Empty typed literal keeps its concrete eltype (was Vector{Any}).
empty_f16 = Float16[]
println(typeof(empty_f16))

# Dot-broadcast constructor narrows the mapped container like Float32.(xs)
# (was Vector{Any} even though the Float16 element values were correct).
bcast_f16 = Float16.([1.0, 2.0])
println(bcast_f16, " :: ", typeof(bcast_f16))

# Range comprehension over a Float16 constructor body: the container narrows to
# Vector{Float16} AND the inline `Float16(i)` body yields Float16 element
# values (Issue #9382 — previously the body value was widened to Float64).
comp_f16 = [Float16(i) for i in 1:3]
println(comp_f16, " :: ", typeof(comp_f16))

# `map` over a bound Float16 array preserves Float16.
mapped_f16 = map(x -> x + Float16(1), Float16[1, 2, 3])
println(mapped_f16, " :: ", typeof(mapped_f16))

# Fold every container-eltype check into the final asserted value so a
# regression in any sibling map flips the fixture result to `false` (Issue
# #9301): previously only the unconditional trailing `true` guarded the file.
all_ok =
    typeof(collect(f16() for _ in 1:3)) === Vector{Float16} &&
    eltype(collect(f16() for _ in 1:3)) === Float16 &&
    typeof(xs[1]) === Float16 &&
    xs == Float16[1, 1, 1] &&
    typeof([Float16(i) for i in 1:3]) === Vector{Float16} &&
    typeof(Vector{Float16}(undef, 3)) === Vector{Float16} &&
    typeof(ys) === Vector{Float16} &&
    typeof(ys[1]) === Float16 &&
    pick(ys) == "f16vec" &&
    typeof(empty_f16) === Vector{Float16} &&
    typeof(bcast_f16) === Vector{Float16} &&
    bcast_f16 == Float16[1.0, 2.0] &&
    # Container narrows to Vector{Float16} AND the inline-constructor element
    # values are Float16 (Issue #9382).
    typeof(comp_f16) === Vector{Float16} &&
    typeof(comp_f16[1]) === Float16 &&
    comp_f16 == Float16[1, 2, 3] &&
    typeof(mapped_f16) === Vector{Float16} &&
    mapped_f16 == Float16[2.0, 3.0, 4.0]

println(all_ok)
all_ok
