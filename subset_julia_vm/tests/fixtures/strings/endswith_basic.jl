# Test endswith - check if string ends with suffix

using Test

@testset "endswith(s, suffix) - Pure Julia (Issue #680)" begin
    @test (endswith("hello world", "world") && endswith("abc", "") && !endswith("abc", "zabc"))
end

true  # Test passed
