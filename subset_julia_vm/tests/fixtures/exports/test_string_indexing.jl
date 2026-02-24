# Test exported string indexing functions

using Test

@testset "String indexing exports" begin
    s = "hello"

    # ncodeunits - get number of code units in string
    @test ncodeunits(s) == 5

    # codeunit - get code unit at index (returns Int64 in SubsetJuliaVM)
    @test codeunit(s, 1) == 104  # 'h' = 0x68 = 104

    # codeunits - get string as byte array
    cu = codeunits(s)
    @test length(cu) == 5

    # nextind - get next valid index
    @test nextind(s, 1) == 2

    # prevind - get previous valid index
    @test prevind(s, 2) == 1

    # thisind - get start of character containing index
    @test thisind(s, 1) == 1
end

true
