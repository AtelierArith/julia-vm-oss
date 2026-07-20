# Fused Int64 arithmetic inside functions can read module-level globals.

using Test

K_8598 = 7
const C_8598 = 5

function add_global_8598(x::Int64)
    return x + K_8598
end

function sub_global_8598(x::Int64)
    return x - K_8598
end

function mul_global_8598(x::Int64)
    return x * K_8598
end

function mod_global_8598(x::Int64)
    return x % K_8598
end

function add_const_global_8598(x::Int64)
    return x + C_8598
end

@testset "fused Int64 global slot reads" begin
    @test add_global_8598(10) == 17
    @test sub_global_8598(10) == 3
    @test mul_global_8598(10) == 70
    @test mod_global_8598(10) == 3
    @test add_const_global_8598(10) == 15
end

true  # Test passed
