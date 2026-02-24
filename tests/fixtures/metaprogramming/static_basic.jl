# Test @static macro - compile-time conditional evaluation

using Test

@testset "@static macro basic tests" begin
    # Test with literal true/false
    result1 = @static if true
        42
    else
        0
    end
    @test result1 == 42

    result2 = @static if false
        42
    else
        0
    end
    @test result2 == 0

    # Test ternary form
    result3 = @static true ? "yes" : "no"
    @test result3 == "yes"

    result4 = @static false ? "yes" : "no"
    @test result4 == "no"
end

@testset "@static with Sys.is* functions" begin
    # Since SubsetJuliaVM targets iOS, Sys.isapple() should be true
    platform = @static Sys.isapple() ? "apple" : "other"
    @test platform == "apple"

    # Sys.isunix() should also be true on iOS
    unix_check = @static Sys.isunix() ? true : false
    @test unix_check == true

    # Sys.iswindows() should be false
    windows_check = @static Sys.iswindows() ? true : false
    @test windows_check == false

    # Sys.islinux() should be false
    linux_check = @static Sys.islinux() ? true : false
    @test linux_check == false
end

true
