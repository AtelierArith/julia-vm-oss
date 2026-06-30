using Test

h_direct_6007(x::String) = "got string: " * x

@testset "direct static method miss raises runtime MethodError (Issue #6007)" begin
    @test h_direct_6007("ok") == "got string: ok"
    @test_throws MethodError h_direct_6007(42)
end

true
