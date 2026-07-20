# Module top-level generator/comprehension lifting must collect nested helpers.

using Test

module GenModuleTopLevel10227
s = sum(i * i for i in 1:3)
d = [x + 1 for x in 1:3]
c = collect(2x for x in 1:3)
end

@testset "module top-level generator helper collection" begin
    @test GenModuleTopLevel10227.s == 14
    @test GenModuleTopLevel10227.d == [2, 3, 4]
    @test GenModuleTopLevel10227.c == [2, 4, 6]
end

true
