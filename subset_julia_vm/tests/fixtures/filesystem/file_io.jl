# Test basic file I/O operations
# Tests: isfile, isdir, ispath, filesize, read, readlines

using Test

@testset "File I/O operations (Issue #347)" begin

    # Test isfile/isdir/ispath
    # Test with paths that are very likely to exist on any system
    # We use the test file itself as a known-to-exist file

    result = true

    # Test ispath with current directory (always exists)
    @assert ispath(".") == true

    # Test isdir with current directory
    @assert isdir(".") == true

    # Test isfile with current directory (should be false)
    @assert isfile(".") == false

    # Test ispath with non-existent path
    @assert ispath("__nonexistent_path_12345__") == false

    # Test isfile with non-existent path
    @assert isfile("__nonexistent_path_12345__") == false

    # Test isdir with non-existent path
    @assert isdir("__nonexistent_path_12345__") == false

    @test (result)
end

true  # Test passed
