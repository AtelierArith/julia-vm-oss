using Test

let a = 1
    @test a == 1
end

let a = 1, b = 2
    @test a + b == 3
end

@testset "let test macro context" begin
    let value = 4
        @test value == 4
    end
end

true
