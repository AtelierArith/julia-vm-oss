using Test

@testset "VersionNumber prerelease data is preserved (Issue #9372)" begin
    a = v"2.0.0-rc1"
    @test string(a) == "2.0.0-rc1"
    @test a != v"2.0.0"
    @test a < v"2.0.0"
    @test v"2.0.0-rc1" < v"2.0.0-rc2"
    @test v"2.0.0-rc2" < v"2.0.0"
end

@testset "VersionNumber show uses literal form (Issue #9371)" begin
    @test sprint(print, v"1.2.3") == "1.2.3"
    @test sprint(show, v"1.2.3") == "v\"1.2.3\""
    @test repr(v"1.2.3") == "v\"1.2.3\""
    @test "$(v"1.2.3")" == "1.2.3"
end

true
