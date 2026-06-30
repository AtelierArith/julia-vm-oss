# Test first/last on Tuple goes through Pure Julia method dispatch (Issue #3734).
# These now resolve via base/tuple.jl method definitions instead of the
# TupleFirst/TupleLast Rust builtins.

using Test

@testset "first((1,2,3)) / last((1,2,3)) Pure Julia dispatch (Issue #3734)" begin
    @test (first((10, 20, 30)) == 10)
    @test (last((10, 20, 30)) == 30)
    @test (first((42,)) == 42)
    @test (last((42,)) == 42)
end

true
