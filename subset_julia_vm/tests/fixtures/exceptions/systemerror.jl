# Test systemerror() function (Issue #448)
# Tests the systemerror function for system error handling

using Test

@testset "systemerror function" begin
    # Test systemerror with errno - should throw
    threw_err = false
    try
        systemerror("syscall", 1)
    catch e
        threw_err = true
        @test isa(e, SystemError)
        @test e.prefix == "syscall"
        @test e.errnum == 1
    end
    @test threw_err

    # Test systemerror with no errno (defaults to 0)
    threw_default = false
    try
        systemerror("operation")
    catch e
        threw_default = true
        @test isa(e, SystemError)
        @test e.errnum == 0
    end
    @test threw_default
end

true
