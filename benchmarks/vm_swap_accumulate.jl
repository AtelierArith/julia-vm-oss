# VM-only benchmark for self-referential destructuring swaps whose targets are
# consumed downstream (Issue #6561).
#
# `swap_sum` carries its state through `a, b = b, (a + b) % 1000003` — a swap
# that lowering desugars to a temporary tuple plus indexed reads — and then
# *uses* the swapped `a` in `s += a`. Before #6561 the `temp[k]` reads returned
# `Any`, so `a` widened off the typed path; that `Any` then poisoned `s += a`
# into a dynamic `DynamicAdd` (method lookup) every iteration and widened the
# accumulator `s` to `Any` as well. With the tuple element types tracked, the
# whole inner loop stays on typed `AddI64`/`StoreI64`. `swap_sum` takes untyped
# parameters so the main compiler emits the dynamic path and runtime lazy
# specialization is exercised.

function swap_sum(a, b, n)
    s = 0
    for _ in 1:n
        a, b = b, (a + b) % 1000003
        s += a
    end
    return s
end

function run_trials(trials, n)
    total = 0
    for _ in 1:trials
        total += swap_sum(1, 1, n)
    end
    return total
end

println(run_trials(150, 2000))
