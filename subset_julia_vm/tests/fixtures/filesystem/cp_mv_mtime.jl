# Test cp, mv, mtime filesystem functions (Issue #457)

using Test

@testset "cp, mv, mtime functions (Issue #457)" begin
    # Test cp - copy file
    test_src = joinpath(tempdir(), "sjvm_test_cp_src_" * string(rand(Int)))
    test_dst = joinpath(tempdir(), "sjvm_test_cp_dst_" * string(rand(Int)))

    # Create source file with content
    touch(test_src)
    @test isfile(test_src)

    # Copy the file
    result = cp(test_src, test_dst)
    @test result == test_dst
    @test isfile(test_dst)
    @test isfile(test_src)  # Source should still exist

    # Clean up
    rm(test_src)
    rm(test_dst)

    # Test mv - move/rename file
    test_mv_src = joinpath(tempdir(), "sjvm_test_mv_src_" * string(rand(Int)))
    test_mv_dst = joinpath(tempdir(), "sjvm_test_mv_dst_" * string(rand(Int)))

    touch(test_mv_src)
    @test isfile(test_mv_src)

    # Move the file
    result = mv(test_mv_src, test_mv_dst)
    @test result == test_mv_dst
    @test isfile(test_mv_dst)
    @test !isfile(test_mv_src)  # Source should no longer exist

    # Clean up
    rm(test_mv_dst)

    # Test mtime - get modification time
    test_mtime = joinpath(tempdir(), "sjvm_test_mtime_" * string(rand(Int)))
    touch(test_mtime)

    # mtime should return a positive float (Unix timestamp)
    mt = mtime(test_mtime)
    @test isa(mt, Float64)
    @test mt > 0.0  # Should be a positive Unix timestamp

    # Modification time should be reasonably recent (within last day)
    # Since we just created the file, it should be close to current time
    @test mt > 1700000000.0  # After 2023

    # Clean up
    rm(test_mtime)
end

true
