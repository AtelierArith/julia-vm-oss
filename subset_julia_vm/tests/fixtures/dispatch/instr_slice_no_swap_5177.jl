# Issue #5177: the dispatch loop no longer swaps each instruction out for a
# temporary `Nop` and back on every cycle; instead it holds an immutable
# snapshot of the shared code slice. This fixture is the behavioral regression
# guard for that refactor, exercising the three properties the old swap/restore
# trick protected:
#
#   1. Hot loops re-execute the *same* code slot many times (loop-back), so the
#      instruction must still be valid on every iteration.
#   2. `eval(Meta.parse(...))` re-enters the interpreter on a fresh frame whose
#      body shares the very code slot currently mid-dispatch; that re-entrant
#      execution must see the real instruction, not a blanked-out slot.
#   3. Runtime specialization (CallSpecialize) appends bytecode to the shared
#      code vector while the loop holds a reference to it; the loop must follow
#      the (possibly reallocated) vector and keep executing correctly.

# (1) Hot loop that re-executes the same instructions thousands of times.
function sum_to(n)
    s = 0
    for i in 1:n
        s += i
    end
    return s
end

@assert sum_to(1000) == 500500
@assert sum_to(1000) == 500500  # second run: snapshot must be reusable

# (3) A type-stable function that the VM may specialize at runtime, called in a
# hot loop so the specialized code path is appended to the shared code vector.
g(x) = 2x + 1

function accumulate_g(n)
    acc = 0
    for k in 1:n
        acc += g(k)
    end
    return acc
end

acc = accumulate_g(500)
@assert acc == 251000
@assert typeof(g(3)) === Int64
@assert typeof(g(2.5)) === Float64

# (2) eval-driven re-entrant dispatch sharing the live code slot.
function h(n)
    if n <= 0
        return 7
    end
    return eval(Meta.parse("h(0)")) + n
end

@assert h(5) == 12  # eval(h(0)) + 5 = 7 + 5

# Cross-check the combined result the way the fixture runner sees it.
@assert sum_to(1000) + acc == 751500

true
