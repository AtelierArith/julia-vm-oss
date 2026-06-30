using Test

h_5984(x::String) = "got string: " * x
g_5984(x::Any) = h_5984(x)

@testset "Any static arg defers single specific method to runtime (Issue #5984)" begin
    @test g_5984("ok") == "got string: ok"
    @test_throws MethodError g_5984(42)
end

true
