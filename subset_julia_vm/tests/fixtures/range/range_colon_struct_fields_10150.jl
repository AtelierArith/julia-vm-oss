using Test

function field_tuple_10150(r)
    names = fieldnames(typeof(r))
    if :step in names
        return (typeof(r), names, getfield(r, :start), getfield(r, :step), getfield(r, :stop))
    end
    return (typeof(r), names, getfield(r, :start), getfield(r, :stop))
end

function local_unit_start_10150()
    r = 1:3
    return getfield(r, :start)
end

function local_step_step_10150()
    r = 1:2:5
    return getfield(r, :step)
end

@testset "colon range literals expose struct fields (#10150)" begin
    r = 1:3
    @test typeof(r) === UnitRange{Int64}
    @test fieldnames(typeof(r)) == (:start, :stop)
    @test getfield(r, :start) === 1
    @test getfield(r, :stop) === 3
    @test field_tuple_10150(r) == (UnitRange{Int64}, (:start, :stop), 1, 3)
    @test local_unit_start_10150() === 1

    s = 1:2:5
    @test typeof(s) === StepRange{Int64, Int64}
    @test fieldnames(typeof(s)) == (:start, :step, :stop)
    @test getfield(s, :start) === 1
    @test getfield(s, :step) === 2
    @test getfield(s, :stop) === 5
    @test field_tuple_10150(s) == (StepRange{Int64, Int64}, (:start, :step, :stop), 1, 2, 5)
    @test local_step_step_10150() === 2

    b = big(1):2:big(5)
    @test typeof(b) === StepRange{BigInt, Int64}
    @test typeof(getfield(b, :start)) === BigInt
    @test getfield(b, :start) == big(1)
    @test getfield(b, :step) === 2
    @test getfield(b, :stop) == big(5)

    u = UInt8(1):UInt16(3)
    @test typeof(u) === UnitRange{UInt16}
    @test getfield(u, :start) === UInt16(1)
    @test getfield(u, :stop) === UInt16(3)

    c = Char(97):Char(99)
    @test typeof(c) === StepRange{Char, Int64}
    @test getfield(c, :start) === 'a'
    @test getfield(c, :step) === 1
    @test getfield(c, :stop) === 'c'
end

@testset "float colon ranges keep StepRangeLen fields (#10150)" begin
    f = 0.0:0.5:1.0
    @test startswith(string(typeof(f)), "StepRangeLen{Float64")
    @test fieldnames(typeof(f)) == (:ref, :step, :len, :offset)
    @test collect(f) == [0.0, 0.5, 1.0]
end

true
