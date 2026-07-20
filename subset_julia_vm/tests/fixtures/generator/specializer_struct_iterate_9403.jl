using Test

function loopsum_9403(iter)
    total = 0
    for value in iter
        total += value
    end
    total
end

struct IterateOnly9403
    stop::Int
end

Base.iterate(iter::IterateOnly9403, state=1) =
    state > iter.stop ? nothing : (state, state + 1)

@testset "specializer keeps protocol iterators off length/getindex (Issue #9403)" begin
    # Seed the runtime specialization with the simple Generator shape first;
    # later struct-backed wrappers must not inherit its representation choice.
    @test loopsum_9403(x for x in 1:3) == 6
    @test loopsum_9403(x + y for x in [1, 2, 3] for y in 1:2) == 21
    @test loopsum_9403(IterateOnly9403(5)) == 15
end

true
