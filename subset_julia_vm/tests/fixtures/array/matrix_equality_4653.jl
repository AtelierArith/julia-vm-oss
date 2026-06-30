using Test

@testset "typed and boxed matrix equality (#4653)" begin
    @test Int64[1 3; 2 4] == Int64[1 3; 2 4]
    @test Int16[1 3; 2 4] == Int16[1 3; 2 4]
    @test Float32[1 3; 2 4] == Float32[1 3; 2 4]
    @test Any["a" "c"; "b" "d"] == Any["a" "c"; "b" "d"]

    @test Int16[1 3; 2 4] != Int16[1 3; 2 5]
    @test Any["a" "c"; "b" "d"] != Any["a" "c"; "b" "x"]
end

true
