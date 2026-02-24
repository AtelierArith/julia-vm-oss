# Test LineNumberNode field access (.line, .file)

using Test

@testset "LineNumberNode field access: .line, .file and constructor" begin

    # Create a LineNumberNode with line and file
    ln = LineNumberNode(42, :myfile)

    # Access .line field
    @assert ln.line == 42

    # Access .file field
    @assert ln.file == :myfile

    # Create LineNumberNode without file (nothing)
    ln2 = LineNumberNode(100)
    @assert ln2.line == 100
    @assert ln2.file === nothing

    # Return value for test harness
    @test (42.0) == 42.0
end

true  # Test passed
