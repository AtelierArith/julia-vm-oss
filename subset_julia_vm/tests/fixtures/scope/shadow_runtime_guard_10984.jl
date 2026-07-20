using Test

# Issue #10984 codex-review hardening: two shapes where the compile-time
# bookkeeping (`locals` type entry / `initialized_locals`) cannot prove the
# outer binding's runtime slot is actually stored, so the shadow
# save/restore must be guarded by a RUNTIME `IsDefined` check.
#
# Shape 1 — nested same-name loops where the OUTER binding is itself a
# constant-step `for` induction variable. The const-step counter path
# registers only a `locals` type (never `initialized_locals`), so the
# original compile-time-only classifier skipped the save entirely and the
# inner loop clobbered the outer loop's live counter mid-iteration:
# sjulia returned (21, [12]) — outer loop terminated after ONE iteration
# with the inner loop's leaked final value — vs upstream (63, [1, 2, 3]).
function nested_induction()
    total = 0
    log = Int[]
    for i in 1:3
        for i in 10:11
            total += i
        end
        push!(log, i)
    end
    return (total, log)
end

result = nested_induction()
@test result[1] == 63
@test result[2] == [1, 2, 3]

# Shape 2 — conditionally-initialized outer local. `initialized_locals` is
# set by straight-line codegen even for an assignment inside an `if`
# branch, so with flag == false the original unguarded save emitted a load
# of a never-stored slot: in a non-slotized frame (the try/catch forces
# the named-variable path here) that crashed with
# `UndefVarError: x not defined` while upstream julia runs fine (the
# `for x` is simply a fresh binding; the dead outer `x` is never read).
function cond_init_shadow(flag)
    if flag
        x = "outer"
    end
    s = 0
    try
        for x in 1:3
            s += x
        end
    catch e
        rethrow(e)
    end
    return s
end

@test cond_init_shadow(true) == 6
@test cond_init_shadow(false) == 6

true
