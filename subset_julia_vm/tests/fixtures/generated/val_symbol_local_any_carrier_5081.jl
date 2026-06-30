using Test

val_symbol_roundtrip_5081(::Val{mode}) where mode = mode

function val_symbol_reassign_5081(::Val{mode}) where mode
    local_mode = mode
    local_mode = 42
    return local_mode
end

@testset "Val symbol local carrier consolidation (Issue #5081)" begin
    @test val_symbol_roundtrip_5081(Val{:fast}()) == :fast
    @test val_symbol_reassign_5081(Val{:safe}()) == 42
end

true
