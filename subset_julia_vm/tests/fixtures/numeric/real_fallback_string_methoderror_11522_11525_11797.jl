# Regression test for Issues #11522, #11525, and #11797: real-number fallbacks must
# retain upstream's ::Real signatures rather than matching arbitrary values.

using Test

@testset "conj(::String) raises MethodError like upstream (Issue #11522)" begin
    e = try
        conj("a")
        nothing
    catch err
        err
    end
    @test typeof(e) == MethodError

    @test conj(3) == 3
    @test conj(-4) == -4
    @test conj(2.5) == 2.5
    @test conj(true) == true
    @test conj(3 + 4im) == 3 - 4im
    @test conj(3.0 + 4.0im) == 3.0 - 4.0im
end

@testset "isreal(::String) raises MethodError like upstream (Issue #11522)" begin
    e = try
        isreal("a")
        nothing
    catch err
        err
    end
    @test typeof(e) == MethodError

    @test isreal(3) == true
    @test isreal(-4) == true
    @test isreal(2.5) == true
    @test isreal(true) == true
    @test isreal(3 + 4im) == false
    @test isreal(3 + 0im) == true
end

@testset "flipsign(::String, ...) raises MethodError like upstream (Issue #11525)" begin
    e = try
        flipsign("a", -1)
        nothing
    catch err
        err
    end
    @test typeof(e) == MethodError

    @test flipsign(3, -1) == -3
    @test flipsign(-3, -1) == 3
    @test flipsign(3, 1) == 3
    @test flipsign(3.5, -2) == -3.5
    @test flipsign(true, -1) == -1
    @test flipsign(3 + 4im, -1) == -3 - 4im
    @test flipsign(3 + 4im, 1) == 3 + 4im
end

@testset "remaining Real fallbacks reject String at dispatch (Issue #11797)" begin
    @test applicable(real, "a") == false
    @test applicable(signbit, "a") == false
    @test applicable(abs, "a") == false
    @test_throws MethodError real("a")
    @test_throws MethodError signbit("a")
    @test_throws MethodError abs("a")

    @test real(3) == 3
    @test signbit(-1) == true
    @test abs(-3) == 3
    @test real(3 + 4im) == 3
    @test abs(3 + 4im) == 5.0
end

println("done")
true
