using Test

@testset "nested rethrow escapes to outer catch (Issue #8435)" begin
    events = String[]
    try
        try
            error("x")
        catch e
            push!(events, "inner")
            rethrow()
            push!(events, "after-rethrow")
        end
    catch e
        push!(events, "outer")
        @test isa(e, ErrorException)
        @test e.msg == "x"
    end

    @test events == ["inner", "outer"]

    thrown_from_catch = ""
    try
        try
            error("original")
        catch e
            throw(ErrorException("replacement"))
        end
    catch e
        @test isa(e, ErrorException)
        @test e.msg == "replacement"
    end

    try
        rethrow()
    catch e
        thrown_from_catch = e.msg
    end
    @test thrown_from_catch == "rethrow() not allowed outside a catch block"

    replacement_from_outside = ""
    try
        rethrow(ErrorException("outside"))
    catch e
        replacement_from_outside = e.msg
    end
    @test replacement_from_outside == "rethrow(exc) not allowed outside a catch block"
end

true
