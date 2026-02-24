# Test mkdir, mkpath, rm, tempdir, tempname, touch, cd, islink functions
# These are filesystem manipulation functions (Issue #457)

using Test

@testset "mkdir and mkpath functions (Issue #457)" begin
    # Test tempdir returns a non-empty string
    tmp = tempdir()
    @test isa(tmp, String)
    @test length(tmp) > 0

    # Test tempname generates unique paths
    t1 = tempname()
    t2 = tempname()
    @test isa(t1, String)
    @test isa(t2, String)
    @test t1 != t2  # Should be unique

    # Test mkdir - create a directory in temp
    test_dir = joinpath(tempdir(), "sjvm_test_mkdir_" * string(rand(Int)))
    result = mkdir(test_dir)
    @test result == test_dir
    @test isdir(test_dir)

    # Test rm - remove the directory
    rm(test_dir)
    @test !isdir(test_dir)

    # Test mkpath - create nested directories
    nested_dir = joinpath(tempdir(), "sjvm_test_mkpath_" * string(rand(Int)), "level1", "level2")
    result = mkpath(nested_dir)
    @test result == nested_dir
    @test isdir(nested_dir)

    # Clean up nested directories
    rm(nested_dir)
    parent1 = dirname(nested_dir)
    rm(parent1)
    parent2 = dirname(parent1)
    rm(parent2)

    # Test touch - create a new file
    test_file = joinpath(tempdir(), "sjvm_test_touch_" * string(rand(Int)))
    result = touch(test_file)
    @test result == test_file
    @test isfile(test_file)

    # Test rm - remove the file
    rm(test_file)
    @test !isfile(test_file)

    # Test islink - should be false for non-existent and regular files
    @test islink("nonexistent_file_xyz") == false
    # Create a temp file to test islink on regular file
    test_file2 = joinpath(tempdir(), "sjvm_test_islink_" * string(rand(Int)))
    touch(test_file2)
    @test islink(test_file2) == false
    rm(test_file2)
end

true
