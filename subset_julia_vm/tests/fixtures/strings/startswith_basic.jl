# Test startswith - check if string starts with prefix

using Test

@testset "startswith(s, prefix) - Pure Julia (Issue #679)" begin
    @test (startswith("hello world", "hello") && startswith("abc", "") && !startswith("abc", "abcd"))
end

true  # Test passed
