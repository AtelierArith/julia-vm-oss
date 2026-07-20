using Test

@testset ";; newline continues the current matrix row (Issue #10519)" begin
    x = [1 2;;
         3 4]
    @test size(x) == (1, 4)
    @test x == [1 2 3 4]

    # The wrap extends only the current physical row. The resulting unequal
    # row widths are accepted by the parser and rejected by hvcat at runtime.
    @test_throws ArgumentError [1 2; 3 4;;
                                5 6]
end

true
