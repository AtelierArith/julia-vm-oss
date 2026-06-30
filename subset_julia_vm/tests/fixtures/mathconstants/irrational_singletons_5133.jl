using Test

Base.@irrational tau5133 6.2831853071795864769 BigFloat("6.2831853071795864769252867665590057683943387987502116419498891846156328125724")

@testset "Irrational singleton constants (Issue #5133)" begin
    @test typeof(tau5133) == Irrational{:tau5133}
    @test tau5133 isa AbstractIrrational
    @test Float64(tau5133) == 6.283185307179586
    @test Float32(tau5133) == Float32(6.283185307179586)
    @test BigFloat(tau5133) == BigFloat("6.2831853071795864769252867665590057683943387987502116419498891846156328125724")
    @test tau5133 == Irrational{:tau5133}()
    @test tau5133 != pi

    @test typeof(pi) == Irrational{:π}
    @test typeof(π) == Irrational{:π}
    @test typeof(ℯ) == Irrational{:ℯ}

    @test pi isa Irrational{:π}
    @test ℯ isa Irrational{:ℯ}
    @test pi == π
    @test !(pi == Float64(pi))
    @test pi != Float64(pi)

    @test Float64(pi) == 3.141592653589793
    @test Float32(pi) == Float32(3.141592653589793)
    @test Float64(ℯ) == 2.718281828459045

    @test typeof(pi + 1) == Float64
    @test pi + 1 == 4.141592653589793
    @test typeof(pi + Float32(1)) == Float32
    @test pi + Float32(1) == Float32(pi) + Float32(1)

    @test BigFloat(pi) == BigFloat("3.141592653589793238462643383279502884197169399375105820974944592307816406286198")
end

true
