# Issue #10130: repeated non-selective `using` of the SAME module must still
# resolve every exported name correctly. `collect_module_metadata()` used to
# re-walk the module's full function-name set once per occurrence; the fix
# memoizes the "import all" merge per module name so repeats are a no-op
# lookup instead of an O(module size) re-scan. This fixture pins the
# correctness side (the perf win is invisible to a single fixture run, but a
# regression in the memoization would drop/miss an imported name).

module DedupModA10130
export helper_10130, other_helper_10130
helper_10130(x) = x + 1
other_helper_10130(x) = x * 2
end

using .DedupModA10130
using .DedupModA10130
using .DedupModA10130

using Test

@testset "duplicate using dedup 10130" begin
    @test helper_10130(41) == 42
    @test other_helper_10130(21) == 42
end

true
