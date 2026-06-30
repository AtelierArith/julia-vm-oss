using Test

@testset "anonymous function in Any tuple array (#4650)" begin
    cases = Any[("x", () -> 1)]

    seen_name = ""
    seen_value = 0
    for (name, f) in cases
        seen_name = name
        seen_value = f()
    end

    @test seen_name == "x"
    @test seen_value == 1
end

true
