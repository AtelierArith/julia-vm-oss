# lstrip/rstrip/strip with predicate function (Issue #2057, #2126)

using Test

@testset "lstrip with predicate" begin
    @test lstrip(isdigit, "123abc") == "abc"
    @test lstrip(isdigit, "abc") == "abc"
    @test lstrip(isdigit, "123") == ""
    @test lstrip(isspace, "  hello") == "hello"
end

@testset "rstrip with predicate" begin
    @test rstrip(isdigit, "abc123") == "abc"
    @test rstrip(isdigit, "abc") == "abc"
    @test rstrip(isdigit, "123") == ""
    @test rstrip(isspace, "hello  ") == "hello"
end

@testset "strip with predicate (Issue #2126)" begin
    @test strip(isdigit, "123abc456") == "abc"
    @test strip(isspace, "  hello  ") == "hello"
    @test strip(c -> c == 'x', "xxxhelloxxx") == "hello"
    @test strip(isdigit, "123") == ""
    @test strip(isdigit, "abc") == "abc"
    @test strip(isdigit, "") == ""
    @test strip(isspace, " ") == ""
    @test strip(isspace, "a") == "a"
    @test strip(isdigit, "123abc") == "abc"
    @test strip(isdigit, "abc456") == "abc"
end

true
