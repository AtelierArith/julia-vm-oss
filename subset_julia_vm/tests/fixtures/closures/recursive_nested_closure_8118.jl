# Issue #8118: a nested function that captures an enclosing local (i.e. is a
# closure) must still be able to call itself recursively and call a
# mutually-recursive sibling. The self/sibling reference must resolve through
# the closure's captured environment, not fail as an "Unknown function".

# MWE 1 — self-recursive closure that also captures `x`.
function f1()
    x = 100
    g(n) = n <= 0 ? x : g(n - 1)
    return g(3)
end

# MWE 2 — mutual (sibling) recursion of nested functions.
function f2()
    a(n) = n <= 0 ? 0 : b(n - 1)
    b(n) = n <= 0 ? 1 : a(n - 1)
    return a(3)
end

# Self-recursion that does real work through the captured value.
function fact_with_capture()
    m = 2
    fact(n) = n <= 1 ? m : n * fact(n - 1)
    return fact(5)          # 5*4*3*2 * m = 240
end

# Mutually-recursive even/odd predicates as nested functions.
function evenodd()
    isev(n) = n == 0 ? true : isod(n - 1)
    isod(n) = n == 0 ? false : isev(n - 1)
    return (isev(10), isod(7), isev(7))
end

# Mutual recursion where the CALLED sibling captures an enclosing local that the
# caller does not reference directly. The caller must transitively capture that
# local so it can reconstruct the sibling closure at the call site (Issue #8118
# residual beyond MWE 2, which captures no enclosing local).
function mutual_callee_captures()
    s = 9
    a(n) = n <= 0 ? 0 : b(n - 1)   # `a` does not reference `s`
    b(n) = n <= 0 ? s : a(n - 1)   # `b` captures the enclosing local `s`
    return a(3)                     # 9
end

# Both siblings capture the enclosing local.
function mutual_both_capture()
    s = 9
    a(n) = n <= 0 ? s : b(n - 1)
    b(n) = n <= 0 ? s : a(n - 1)
    return a(3)                     # 9
end

# Three-way mutual recursion whose members capture an enclosing local.
function mutual_three_way()
    s = 0
    p(n) = n <= 0 ? s : q(n - 1)
    q(n) = n <= 0 ? s + 1 : r(n - 1)
    r(n) = n <= 0 ? s + 2 : p(n - 1)
    return p(7)                     # 1
end

# Regression guards: a non-recursive escaping closure and a closure that
# captures a function argument must still work unchanged.
function counter()
    c = 0
    step() = (c += 1; c)
    return step
end
makeapply(fn) = x -> fn(x)

h = counter()
inc = makeapply(y -> y + 1)

f1() == 100 &&
    f2() == 1 &&
    fact_with_capture() == 240 &&
    evenodd() == (true, true, false) &&
    mutual_callee_captures() == 9 &&
    mutual_both_capture() == 9 &&
    mutual_three_way() == 1 &&
    (h(), h(), h()) == (1, 2, 3) &&
    inc(41) == 42
