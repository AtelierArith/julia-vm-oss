using Test

function const_prop_ssa_phi_fold_8440(flag)
    x = 0
    if flag
        x = 41
    else
        x = 41
    end
    x + 1
end

@testset "SSA phi fold parity (Issue #8440)" begin
    @test const_prop_ssa_phi_fold_8440(true) == 42
    @test const_prop_ssa_phi_fold_8440(false) == 42
end

true
