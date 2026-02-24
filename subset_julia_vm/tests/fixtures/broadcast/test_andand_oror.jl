# Test AndAnd/OrOr short-circuit broadcast operators (Issue #2545)
# Reference: julia/base/broadcast.jl L194-211

using Test

# andand/oror are plain functions wrapping && and ||
function andand(a, b)
    return a && b
end

function oror(a, b)
    return a || b
end

@testset "AndAnd/OrOr scalar" begin
    # andand basic
    @test andand(true, true) == true
    @test andand(true, false) == false
    @test andand(false, true) == false
    @test andand(false, false) == false

    # oror basic
    @test oror(true, true) == true
    @test oror(true, false) == true
    @test oror(false, true) == true
    @test oror(false, false) == false
end

@testset "AndAnd/OrOr broadcast with arrays" begin
    # .&& broadcast: element-wise &&
    a = [true, false, true, false]
    b = [true, true, false, false]
    result_and = a .&& b
    @test result_and[1] == true
    @test result_and[2] == false
    @test result_and[3] == false
    @test result_and[4] == false

    # .|| broadcast: element-wise ||
    result_or = a .|| b
    @test result_or[1] == true
    @test result_or[2] == true
    @test result_or[3] == true
    @test result_or[4] == false
end

@testset "AndAnd/OrOr broadcast with scalar" begin
    # Array .&& scalar
    a = [true, false, true]
    @test (a .&& true) == [true, false, true]
    @test (a .&& false) == [false, false, false]

    # Array .|| scalar
    @test (a .|| true) == [true, true, true]
    @test (a .|| false) == [true, false, true]
end

true
