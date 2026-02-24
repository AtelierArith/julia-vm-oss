# Test that Bool argument dispatches to Bool method, not Int64 method
# Issue #1441: Method dispatch prefers Integer over Bool

using Test

# Define overloaded functions with Bool and Int64 variants
function dispatch_test(b::Bool)
    return 1  # Bool version
end

function dispatch_test(n::Int64)
    return 2  # Int64 version
end

@testset "Bool vs Int64 dispatch" begin
    # Bool argument should dispatch to Bool method
    @test dispatch_test(true) == 1
    @test dispatch_test(false) == 1
    
    # Int64 argument should dispatch to Int64 method
    @test dispatch_test(42) == 2
    @test dispatch_test(0) == 2
end

true
