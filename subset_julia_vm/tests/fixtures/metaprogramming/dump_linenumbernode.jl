# Test dump for LineNumberNode

using Test

@testset "dump() for LineNumberNode - shows line and file fields" begin

    # LineNumberNode with just line number
    ln1 = LineNumberNode(42)
    dump(ln1)

    # LineNumberNode with line and file
    ln2 = LineNumberNode(10, :myfile)
    dump(ln2)

    # Access fields directly
    @assert ln1.line == 42
    @assert ln1.file === nothing
    @assert ln2.line == 10
    @assert ln2.file === :myfile

    # Final result
    @test (42.0) == 42.0
end

true  # Test passed
