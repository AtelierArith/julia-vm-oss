using Test

@testset "VersionNumber equality" begin
    @test VersionNumber(1, 2, 3) == VersionNumber(1, 2, 3)
    @test VersionNumber(1, 0, 0) != VersionNumber(1, 0, 1)
    @test VersionNumber(0, 1, 0) != VersionNumber(1, 0, 0)
end

@testset "VersionNumber comparison operators (Issue #7529)" begin
    @test v"1.2.0" >= v"1.1.0"
    @test v"1.2.0" > v"1.1.9"
    @test v"1.2.0" <= v"1.2.0"
    @test v"1.2.0" < v"1.2.1"
    @test !(v"1.0.0" >= v"2.0.0")
    @test VERSION >= v"0.1.0"
end

@testset "VersionNumber field access patterns" begin
    v = VersionNumber(3, 5, 7)
    @test v.major == 3
    @test v.minor == 5
    @test v.patch == 7
    @test v.major + v.minor + v.patch == 15
end

@testset "VersionNumber string representation" begin
    @test string(VersionNumber(1, 2, 3)) == "1.2.3"
    @test string(VersionNumber(0, 1, 0)) == "0.1.0"
    @test string(VersionNumber(10, 20, 30)) == "10.20.30"
end

@testset "VersionNumber print form (Issue #9327)" begin
    # Regression: previously `string`/`print` rendered the default struct
    # constructor form `VersionNumber(1, 2, 3)` instead of the dotted
    # `major.minor.patch`. Upstream `print(io, ::VersionNumber)` emits the
    # dotted form for `string`/`print`/`println`/`sprint`.
    @test string(v"1.2.3") == "1.2.3"
    @test sprint(print, v"1.2.3") == "1.2.3"
    @test sprint(print, VersionNumber(2, 0, 0)) == "2.0.0"
    let io = IOBuffer()
        print(io, v"3.4.5")
        @test String(take!(io)) == "3.4.5"
    end
    @test string(VersionNumber(1)) == "1.0.0"
    @test string(VersionNumber(2, 3)) == "2.3.0"
end

@testset "VersionNumber default args" begin
    v1 = VersionNumber(1)
    @test v1.major == 1
    @test v1.minor == 0
    @test v1.patch == 0

    v2 = VersionNumber(2, 3)
    @test v2.major == 2
    @test v2.minor == 3
    @test v2.patch == 0
end

true
