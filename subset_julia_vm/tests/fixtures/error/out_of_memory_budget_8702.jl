using Test

function caught_out_of_memory_8702(e)
    return e isa OutOfMemoryError &&
        typeof(e) == OutOfMemoryError &&
        sprint(showerror, e) == "OutOfMemoryError()"
end

@testset "budgeted known-size allocations raise OutOfMemoryError (Issue #8702)" begin
    allocation = try
        zeros(1024)
        nothing
    catch e
        e
    end

    growth = try
        a = zeros(512)
        push!(a, 1.0)
        nothing
    catch e
        e
    end

    # Keep this clause-local so `chunk` travels through Any-typed dynamic `*`;
    # that path must enforce the same budget as typed StringConcat (Issue #11308).
    concat = try
        chunk = "x" ^ 1024
        chunk * chunk * chunk * chunk * chunk
        nothing
    catch e
        e
    end

    @test caught_out_of_memory_8702(allocation)
    @test caught_out_of_memory_8702(growth)
    @test caught_out_of_memory_8702(concat)
end

true
