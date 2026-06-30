# Issue #7245 regression guard: `using <stdlib>` inside a user-defined module
# must keep working. Statistics was the known-good control case that already
# worked before the #7245 fix; this fixture keeps it green so the fix does not
# regress nested-module stdlib loading.

using Test
using Statistics

module StatsMod
using Statistics
mmean(x) = mean(x)
mstd(x) = std(x)
export mmean, mstd
end
using .StatsMod

@testset "using Statistics inside a user module still works (Issue #7245)" begin
    @test StatsMod.mmean([1.0, 2.0, 3.0]) == 2.0
    @test StatsMod.mstd([2.0, 4.0, 6.0]) == 2.0
end

true  # Test passed
