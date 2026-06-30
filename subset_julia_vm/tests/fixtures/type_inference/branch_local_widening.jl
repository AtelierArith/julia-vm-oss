# Branches that assign different incompatible types must produce a value
# whose runtime type matches the branch taken (not the last syntactic branch).
# Issues #3535 and #3536

using Test

function f3535(c)
    x = 1
    if c
        x = "one"
    end
    return x
end

function f3536(c)
    if c
        x = 1
    else
        x = "s"
    end
    return x
end

@testset "Branch-local type widening preserves both branches" begin
    @test f3535(false) == 1
    @test f3535(true) == "one"

    @test f3536(true) == 1
    @test f3536(false) == "s"
end

true
