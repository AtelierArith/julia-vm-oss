# sort on string arrays and string comparison operators (Issue #2025)

using Test

@testset "String comparison operators (Issue #2025)" begin
    @test ("apple" < "banana") == true
    @test ("banana" > "apple") == true
    @test ("apple" <= "apple") == true
    @test ("apple" >= "apple") == true
    @test ("apple" <= "banana") == true
    @test ("cherry" >= "banana") == true
    @test ("abc" < "abd") == true
    @test ("abc" < "abc") == false
end

@testset "sort on string arrays (Issue #2025)" begin
    @test sort(["banana", "apple", "cherry"]) == ["apple", "banana", "cherry"]
    @test sort(["cherry", "banana", "apple", "date"]) == ["apple", "banana", "cherry", "date"]
    @test sort(["cherry", "banana", "apple", "date"], rev=true) == ["date", "cherry", "banana", "apple"]
end

@testset "issorted for string arrays" begin
    @test issorted(["apple", "banana", "cherry"]) == true
    @test issorted(["cherry", "banana", "apple"]) == false
end

true
