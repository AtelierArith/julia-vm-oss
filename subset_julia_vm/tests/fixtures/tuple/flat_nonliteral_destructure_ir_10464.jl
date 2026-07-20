using Test

const CALLS_10464 = Ref(0)

function pair_10464()
    CALLS_10464[] += 1
    (11, 22)
end

function plain_tail_10464()
    (a, b) = pair_10464()
end

function statement_use_10464()
    (a, b) = pair_10464()
    a + b
end

function begin_tail_10464()
    begin
        (a, b) = pair_10464()
    end
end

function let_tail_10464()
    let
        (a, b) = pair_10464()
    end
end

function try_tail_10464()
    try
        (a, b) = pair_10464()
    catch
        (-1, -1)
    end
end

array_rhs_10464() = [31, 32, 33]
range_rhs_10464() = 41:44

function array_destructure_10464()
    (a, b) = array_rhs_10464()
    (a, b)
end

function range_destructure_10464()
    (a, b) = range_rhs_10464()
    (a, b)
end

array_tail_10464() = ((a, b) = array_rhs_10464())
range_tail_10464() = ((a, b) = range_rhs_10464())

function extra_rhs_10464()
    (a, b) = (51, 52, 53)
    (a, b)
end

extra_tail_10464() = ((a, b) = (51, 52, 53))

const EXTRA_EFFECTS_10464 = Ref(0)
function extra_effect_tail_10464()
    (a, b) = (
        (EXTRA_EFFECTS_10464[] += 1; 71),
        (EXTRA_EFFECTS_10464[] += 1; 72),
        (EXTRA_EFFECTS_10464[] += 1; 73),
    )
end

function short_tail_throws_10464()
    try
        (a, b, c) = (81, 82)
        false
    catch err
        err isa BoundsError
    end
end

struct PairIterator10464
    first::Int
    second::Int
end
Base.iterate(it::PairIterator10464) = (it.first, 2)
Base.iterate(it::PairIterator10464, state::Int) = state == 2 ? (it.second, 3) : nothing

function generator_destructure_10464()
    (a, b) = (x * 2 for x in 1:4)
    (a, b)
end

function custom_iterator_destructure_10464()
    (a, b) = PairIterator10464(91, 92)
    (a, b)
end

function partition_destructure_10464()
    (a, b) = Iterators.partition(1:5, 2)
    (a, b)
end

function compiler_temp_collision_10464()
    __destructure_1 = 101
    (a, b) = 93:94
    (__destructure_1, a, b)
end

function short_rhs_throws_10464()
    try
        (a, b, c) = (61, 62)
        false
    catch err
        err isa BoundsError
    end
end

@testset "flat nonliteral destructuring explicit IR (Issue #10464)" begin
    CALLS_10464[] = 0
    @test plain_tail_10464() == (11, 22)
    @test CALLS_10464[] == 1

    @test statement_use_10464() == 33
    @test begin_tail_10464() == (11, 22)
    @test let_tail_10464() == (11, 22)
    @test try_tail_10464() == (11, 22)
    @test CALLS_10464[] == 5
    @test array_destructure_10464() == (31, 32)
    @test range_destructure_10464() == (41, 42)
    @test array_tail_10464() == [31, 32, 33]
    @test range_tail_10464() == 41:44
    @test extra_rhs_10464() == (51, 52)
    @test extra_tail_10464() == (51, 52, 53)
    EXTRA_EFFECTS_10464[] = 0
    @test extra_effect_tail_10464() == (71, 72, 73)
    @test EXTRA_EFFECTS_10464[] == 3
    @test short_rhs_throws_10464()
    @test short_tail_throws_10464()
    @test generator_destructure_10464() == (2, 4)
    @test custom_iterator_destructure_10464() == (91, 92)
    @test partition_destructure_10464() == ([1, 2], [3, 4])
    @test compiler_temp_collision_10464() == (101, 93, 94)
end

true
