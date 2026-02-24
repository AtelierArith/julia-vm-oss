# Phase 3 begin/end block unquoting

using Test

function block_gen(x)
    if @generated
        :(begin
            y = x + 1
            y * 2
        end)
    else
        begin
            y = x + 1
            y * 2
        end
    end
end

@testset "Phase 3 begin block" begin
    @test block_gen(5) == 12  # (5+1)*2 = 12
    @test block_gen(0) == 2   # (0+1)*2 = 2
end

true
