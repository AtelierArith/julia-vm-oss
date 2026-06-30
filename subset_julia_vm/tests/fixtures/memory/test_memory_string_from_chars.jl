using Test

@testset "String from Memory{Char} direct storage" begin
    m = Memory{Char}(undef, 5)
    m[1] = 'h'
    m[2] = 'e'
    m[3] = 'l'
    m[4] = 'l'
    m[5] = 'o'

    @test String(m) == "hello"

    empty = Memory{Char}(undef, 0)
    @test String(empty) == ""

    ints = Memory{Int64}(undef, 2)
    ints[1] = 65
    ints[2] = 66

    failed = false
    try
        String(ints)
    catch err
        failed = true
    end
    @test failed
end

true
