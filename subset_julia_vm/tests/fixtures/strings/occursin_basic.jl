# Test occursin - check if needle appears in haystack

using Test

@testset "occursin(needle, haystack) - Pure Julia (Issue #681)" begin
    @test (occursin("world", "hello world") && occursin("", "abc") && !occursin("xyz", "abc") && !occursin("abcd", "abc"))
end

true  # Test passed
