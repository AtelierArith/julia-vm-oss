using Test

# Issue #5725: a range searched as an ELEMENT of a collection (`(1:3) in [1:3,...]`,
# findfirst/findall over an array of ranges) returned false — the `in` builtin's
# value-equality helper had no Range arm. Ranges compare element-wise.

@testset "range as a collection element (Issue #5725)" begin
    @test ((1:3) in [1:3, 4:6]) == true
    @test ((1:3) in [4:6, 7:9]) == false
    @test ((1:2:9) in [1:2:9, 2:2:10]) == true

    # element-wise: 1:3 equals 1:1:3 (UnitRange vs StepRange, same elements)
    @test ((1:3) in [1:1:3, 4:6]) == true

    # all empty ranges are equal
    @test ((1:0) in [5:3]) == true

    # tuple container
    @test ((1:3) in (1:3, 4:6)) == true

    # findfirst / findall over arrays of ranges
    @test findfirst(==(1:3), [4:6, 1:3]) == 2
    @test findall(==(2:4), [1:3, 2:4, 2:4]) == [2, 3]
end

true
