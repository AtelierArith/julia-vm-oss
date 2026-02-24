# Test reflection methods for LineNumberNode
# nfields and fieldnames should work for LineNumberNode

using Test

@testset "Reflection methods (nfields, fieldnames) for LineNumberNode" begin

    ln = LineNumberNode(42, :myfile)

    # nfields should return 2 (line, file)
    n = nfields(ln)
    println(n)  # Should print 2

    # fieldnames should return (:line, :file)
    names = fieldnames(LineNumberNode)
    println(names)  # Should print (:line, :file)

    # Verify field names
    name1 = names[1]
    name2 = names[2]
    println(name1)  # Should print :line
    println(name2)  # Should print :file

    @test (n) == 2.0
end

true  # Test passed
