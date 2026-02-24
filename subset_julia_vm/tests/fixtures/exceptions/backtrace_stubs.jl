# Test backtrace stub functions
# Issue #448: Error handling (エラー処理)
#
# These functions return empty arrays as stubs since full backtrace
# support requires VM-level stack inspection.

using Test

@testset "Backtrace stubs" begin
    # Test backtrace() returns an array
    bt = backtrace()
    @test isa(bt, Array)
    @test length(bt) == 0  # Stub returns empty array

    # Test catch_backtrace() returns an array
    cbt = catch_backtrace()
    @test isa(cbt, Array)
    @test length(cbt) == 0

    # Test stacktrace() returns an array
    st = stacktrace()
    @test isa(st, Array)
    @test length(st) == 0

    # Test current_exceptions() returns an array
    excs = current_exceptions()
    @test isa(excs, Array)
    @test length(excs) == 0

    # Test stacktrace with argument
    st2 = stacktrace(Int64[])
    @test isa(st2, Array)
    @test length(st2) == 0
end

true
