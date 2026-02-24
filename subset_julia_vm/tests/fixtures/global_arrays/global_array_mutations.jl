using Test

# Global arrays mutated via push!/pop!/pushfirst!/popfirst!/insert!/deleteat! inside functions
# Issue #3121: StoreArray inside functions caused slotization to shadow global arrays

const ACCUM = Int64[]
const LOG3 = [10, 20, 30]
const FRONT = [1, 2, 3]

function accumulate_val(v)
    push!(ACCUM, v)
end

function pop_last()
    pop!(LOG3)
end

function push_to_front(v)
    pushfirst!(FRONT, v)
end

function pop_from_front()
    popfirst!(FRONT)
end

const INS_ARR = [1, 3, 4]

function insert_middle(v)
    insert!(INS_ARR, 2, v)
end

const DEL_ARR = [10, 99, 20]

function delete_second()
    deleteat!(DEL_ARR, 2)
end

@testset "Global array mutations via functions (Issue #3121)" begin
    @testset "push! on global array" begin
        accumulate_val(5)
        accumulate_val(10)
        accumulate_val(15)
        @test length(ACCUM) == 3
        @test ACCUM[1] == 5
        @test ACCUM[2] == 10
        @test ACCUM[3] == 15
    end

    @testset "pop! on global array" begin
        val = pop_last()
        @test val == 30.0
        @test length(LOG3) == 2
    end

    @testset "pushfirst! on global array" begin
        push_to_front(0)
        @test FRONT[1] == 0
        @test FRONT[2] == 1
        @test length(FRONT) == 4
    end

    @testset "popfirst! on global array" begin
        val = pop_from_front()
        @test val == 0.0
        @test FRONT[1] == 1
        @test length(FRONT) == 3
    end

    @testset "insert! on global array" begin
        insert_middle(2)
        @test INS_ARR[1] == 1
        @test INS_ARR[2] == 2
        @test INS_ARR[3] == 3
        @test length(INS_ARR) == 4
    end

    @testset "deleteat! on global array" begin
        delete_second()
        @test length(DEL_ARR) == 2
        @test DEL_ARR[1] == 10
        @test DEL_ARR[2] == 20
    end
end

true
