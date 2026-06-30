using Test

function match(a, b, c)
    return a + b + c
end

@testset "user match methods dispatch before regex builtin guard (Issue #7502)" begin
    @test match(1, 2, 3) == 6
end

true
