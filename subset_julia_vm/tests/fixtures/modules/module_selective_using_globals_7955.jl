using Test

module SelectiveUsingGlobals7955

export exported_global_7955, ExportedAlias7955

exported_global_7955 = 11
hidden_global_7955 = 22
const ExportedAlias7955 = Int64
const HiddenAlias7955 = Int64

end

using .SelectiveUsingGlobals7955: hidden_global_7955, HiddenAlias7955

@testset "selective using module globals (PR #7955)" begin
    @test hidden_global_7955 == 22
    @test HiddenAlias7955(33) == 33
    @test typeof(HiddenAlias7955(33)) === Int64
    @test SelectiveUsingGlobals7955.exported_global_7955 == 11
    @test SelectiveUsingGlobals7955.ExportedAlias7955(44) == 44
    @test_throws UndefVarError exported_global_7955
    @test_throws UndefVarError ExportedAlias7955(44)
end

true
