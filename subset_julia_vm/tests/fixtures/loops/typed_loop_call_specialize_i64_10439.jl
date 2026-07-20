# Issue #10439: call-site specialization of untyped callee bodies inside typed
# loops. A loop-bodied *untyped* helper called from a typed loop now inlines as
# TypedLoopOp::CallSpecializeI64Function, running the SAME runtime-specialized
# I64 body the generic CallSpecializeI64Slots dispatch runs. This fixture pins
# that the new path is bit-for-bit faithful to (a) the identical logic written
# with a *typed* helper (which uses the #10309 CallI64Function path) and (b)
# upstream Julia, across negative modulo, zero, and Int64 overflow boundaries.
#
# The hot loops compare `helper(a,b) == <literal>` — the exact shape the typed
# loop recognizer inlines (a compare against a parameter is not inlined, so the
# comparison target must be a literal). `a` sweeps negatives; `b` stays a
# positive (or, in the *_negdiv twins, negative) divisor.

# Untyped helper. The `while` body blocks straight-line inlining, so the caller
# keeps a real CallSpecializeI64Slots call — exactly the mygcd shape. Exercises
# truncated rem (sign follows the dividend) and wrapping *, +, -.
function umix(a, b)
    r = a
    s = 0
    k = 0
    while k < 3
        r = r % b
        s = s + r * a - b
        r = r + a
        k = k + 1
    end
    s
end

# Typed twin: identical body, concrete Int64 signature.
function tmix(a::Int64, b::Int64)::Int64
    r = a
    s = 0
    k = 0
    while k < 3
        r = r % b
        s = s + r * a - b
        r = r + a
        k = k + 1
    end
    s
end

# Count matches against literal targets over a range spanning negatives. Two
# spellings each: untyped helper (new CallSpecializeI64Function path) vs typed
# helper (existing #10309 CallI64Function path). Positive and negative divisors.
function u_eq0(lo, hi)
    c = 0
    for a in lo:hi
        for b in 1:hi
            if umix(a, b) == 0
                c += 1
            end
        end
    end
    c
end
function t_eq0(lo, hi)
    c = 0
    for a in lo:hi
        for b in 1:hi
            if tmix(a, b) == 0
                c += 1
            end
        end
    end
    c
end
function u_eq_neg(lo, hi)
    c = 0
    for a in lo:hi
        for b in 1:hi
            if umix(a, b) == -60
                c += 1
            end
        end
    end
    c
end
function t_eq_neg(lo, hi)
    c = 0
    for a in lo:hi
        for b in 1:hi
            if tmix(a, b) == -60
                c += 1
            end
        end
    end
    c
end
function u_eq0_negdiv(lo, hi)
    c = 0
    for a in lo:hi
        for b in -hi:-1
            if umix(a, b) == 0
                c += 1
            end
        end
    end
    c
end
function t_eq0_negdiv(lo, hi)
    c = 0
    for a in lo:hi
        for b in -hi:-1
            if tmix(a, b) == 0
                c += 1
            end
        end
    end
    c
end

# 2. Int64 overflow / boundary probes: wrapping arithmetic and the typemin % -1
#    case (which bails to the generic path) must still match the typed twin.
function edges_match()
    edges = true
    edges &= umix(typemin(Int64), -1) == tmix(typemin(Int64), -1)
    edges &= umix(typemax(Int64), 7) == tmix(typemax(Int64), 7)
    edges &= umix(typemin(Int64), 3) == tmix(typemin(Int64), 3)
    edges &= umix(typemax(Int64), -1) == tmix(typemax(Int64), -1)
    edges &= umix(0, 5) == tmix(0, 5)
    edges &= umix(5, 1) == tmix(5, 1)
    edges
end

# 1. new untyped-in-loop path == typed-in-loop path exactly (both divisor signs).
same_pos    = u_eq0(-30, 30) == t_eq0(-30, 30)
same_neg    = u_eq_neg(-30, 30) == t_eq_neg(-30, 30)
same_negdiv = u_eq0_negdiv(-30, 30) == t_eq0_negdiv(-30, 30)
# 3. absolute regression pins (values verified against upstream `julia`).
pins = u_eq0(-30, 30) == 10 &&
       u_eq0_negdiv(-30, 30) == 0 &&
       umix(-7, 3) == 12 &&
       umix(7, 3) == 12

println(same_pos)
println(same_neg)
println(same_negdiv)
println(edges_match())
println(pins)

# Final value is the assertion the fixture harness checks (all checks ANDed).
same_pos && same_neg && same_negdiv && edges_match() && pins
