# Issue #7600: a global / const / builtin constant referenced from a
# second-level (or deeper) nested closure must resolve to the global binding,
# exactly as a single-level closure does and as upstream Julia does. The bug
# added the global to the inner closure's captured_vars, failing at runtime
# with `UndefVarError: Cannot capture undefined variable: <name>` once the
# closure was nested at depth >= 2.

using Test

const K = 100
G = 7

outer(f) = f(10)
inner(g) = g(20)

# 3-level helpers (a + b + c flows through two intermediate do-blocks)
o1(f) = f(1)
o2(g) = g(2)
o3(h) = h(3)

@testset "nested closure global/const/builtin capture (Issue #7600)" begin
    # E) single-level control still works
    @test (inner() do b; b + pi; end) == 23.141592653589793

    # F) two-level nested, references the builtin `pi` only
    @test (outer() do a; inner() do b; b + pi; end; end) == 23.141592653589793

    # Unicode `π` behaves identically to `pi`
    @test (outer() do a; inner() do b; b + π; end; end) == 23.141592653589793

    # D) two-level nested, outer local `a` + builtin `pi`
    @test (outer() do a; inner() do b; a + b + pi; end; end) == 33.1415926535898

    # G) two-level nested, user `const K`
    @test (outer() do a; inner() do b; a + b + K; end; end) == 130

    # H) two-level nested, non-const global `G`
    @test (outer() do a; inner() do b; a + b + G; end; end) == 37

    # Three-level nesting: outer locals a, b plus a const and a builtin all
    # resolve correctly through two intermediate do-blocks.
    @test (o1() do a; o2() do b; o3() do c; a + b + c + K + pi; end; end; end) ==
          109.1415926535898
end

true
