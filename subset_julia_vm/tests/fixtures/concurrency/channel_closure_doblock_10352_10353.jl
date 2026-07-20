# Channel dispatch and do-block producer gaps (Issues #10352 / #10353)
#
# - take!(c) inside a closure/@async body must dispatch to the pure-Julia
#   Channel method instead of the builtin IOBuffer take! (Issue #10352).
# - A do-block producer body may contain `for` (and other statement forms),
#   and the typed Channel{T}(func, sz) constructor exists (Issue #10353).
# collect(::Channel) with a pending producer is tracked separately
# (Issue #11417) and deliberately not exercised here.

using Test

@testset "take! dispatches to Channel inside closures and tasks" begin
    c = Channel(2)
    put!(c, 1)
    put!(c, 2)
    close(c)
    f = () -> take!(c)
    @test f() == 1
    t = @async take!(c)
    @test fetch(t) == 2
end

@testset "do-block producer with a for-statement body" begin
    c = Channel(10) do ch
        for i in 1:3
            put!(ch, i)
        end
    end
    @test take!(c) == 1
    @test take!(c) == 2
    @test take!(c) == 3

    got = Int[]
    c2 = Channel(4) do ch
        for i in 1:2
            put!(ch, 10 * i)
        end
    end
    for x in c2
        push!(got, x)
    end
    @test got == [10, 20]
end

@testset "typed Channel{T}(func, sz) producer" begin
    c = Channel{Int}(2) do ch
        put!(ch, 10)
    end
    @test take!(c) == 10

    c3 = Channel{Int}(3) do ch
        for i in 1:3
            put!(ch, i * i)
        end
    end
    @test take!(c3) == 1
    @test take!(c3) == 4
    @test take!(c3) == 9
end

true
