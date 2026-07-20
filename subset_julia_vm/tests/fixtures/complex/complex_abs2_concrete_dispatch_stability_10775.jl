# Issue #10775: sjulia's method resolution nondeterministically matched a
# concrete `Complex{Float64}` / `Complex{Float32}` method parameter against a
# `Complex{Int64}` (or other non-Float) argument, in a process-seed-dependent
# (HashMap iteration order) way — `abs2(Complex(2, 3))` returned
# `13.0::Float64`/`Float32` instead of `13::Int64` in roughly 1 of every 3
# processes on unmodified `main` (commit 6d4b174a0), even though
# `Base.abs2` has a fully general `abs2(z::Complex{T}) where {T<:Real}`
# fallback plus the concrete `abs2(z::Complex{Float64})` /
# `abs2(z::Complex{Float32})` overloads that must never match a
# `Complex{Int64}` argument.
#
# A single run cannot catch a process-seed-dependent bug (that is exactly why
# it went unnoticed for a long time — see the issue's "why existing tests
# missed it" analysis). The real regression guard is the multi-process sweep:
# `scripts/dispatch_seed_sweep.sh complex` runs every fixture in this category
# (this one included) across N fresh processes and fails if any process's
# output differs from the first. This fixture pins the *values* the sweep
# must keep stable.
z_int = Complex(2, 3)
r_int = abs2(z_int)
println(typeof(z_int), " -> abs2 = ", r_int, " :: ", typeof(r_int))

z_i32 = Complex(Int32(2), Int32(3))
r_i32 = abs2(z_i32)
println(typeof(z_i32), " -> abs2 = ", r_i32, " :: ", typeof(r_i32))

z_bool = Complex(true, false)
r_bool = abs2(z_bool)
println(typeof(z_bool), " -> abs2 = ", r_bool, " :: ", typeof(r_bool))

z_f64 = Complex(2.0, 3.0)
r_f64 = abs2(z_f64)
println(typeof(z_f64), " -> abs2 = ", r_f64, " :: ", typeof(r_f64))

z_f32 = Complex{Float32}(2.0f0, 3.0f0)
r_f32 = abs2(z_f32)
println(typeof(z_f32), " -> abs2 = ", r_f32, " :: ", typeof(r_f32))

w = Complex(2, 3) / Complex(2, 3)
println("z/z = ", w, " :: ", typeof(w))

(typeof(r_int) == Int64 && r_int == 13) &&
    (typeof(r_i32) == Int32 && r_i32 == Int32(13)) &&
    (typeof(r_bool) == Int64 && r_bool == 1) &&
    (typeof(r_f64) == Float64 && r_f64 == 13.0) &&
    (typeof(r_f32) == Float32 && r_f32 == 13.0f0) &&
    (typeof(w) == Complex{Float64} && w == 1.0 + 0.0im)
