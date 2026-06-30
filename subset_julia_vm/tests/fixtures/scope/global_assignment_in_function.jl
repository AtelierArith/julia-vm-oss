# Test `global x` declarations inside a function body.
#
# Issue #5548: `global n; n = n + 1` (read-modify-write) raised UndefVarError,
#              and a bare `global n; n = 99` did not propagate to the top-level
#              binding.
# Issue #5549: `global counter += 1` inside a function raised UndefVarError and
#              did not update the top-level binding.
#
# A `global x` declaration must route both reads and writes of `x` to the
# top-level (module) binding, matching upstream Julia. The bindings below live
# at module top level on purpose: a `@testset` body is a *local* scope, so a
# `global` declaration there would refer to an unset module binding.

using Test

# --- Issue #5548: read-modify-write through a global declaration ---
n = 0
function inc()
    global n
    n = n + 1
    return n
end
@test inc() == 1
@test n == 1
@test inc() == 2
@test n == 2

# --- Issue #5548: write-only assignment propagates to the global binding ---
m = 0
function set_m()
    global m
    m = 99
    return m
end
@test set_m() == 99
@test m == 99

# --- Issue #5549: compound assignment through a global declaration ---
counter = 0
function source()
    global counter += 1
    21
end
@test source() == 21
@test counter == 1
@test source() == 21
@test counter == 2

# --- read-only access to a global still works (regression guard) ---
k = 5
function read_k()
    global k
    return k
end
@test read_k() == 5

# --- `global` inside a nested control-flow block (pre-scan recursion) ---
acc = 0
function run_loop()
    for i in 1:3
        global acc
        acc += i
    end
    return acc
end
@test run_loop() == 6
@test acc == 6

# --- `global` inside a closure (free-variable analysis must not capture it) ---
hits = 0
function make_counter()
    function bump()
        global hits
        hits += 1
        return hits
    end
    return bump
end
counter_fn = make_counter()
@test counter_fn() == 1
@test counter_fn() == 2
@test hits == 2

println("all global-in-function tests passed")
true  # Test passed
