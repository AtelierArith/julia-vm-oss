using Test

eval(:(eval_defined_catch_binding_11075() = begin
    try
        error("boom")
    catch caught_11075
        nothing
    end
    caught_11075
end))

@generated function generated_catch_binding_11075(x)
    quote
        try
            error("boom")
        catch caught_11075
            nothing
        end
        caught_11075
    end
end

function eval_defined_catch_binding_is_scoped_11075()
    try
        Base.invokelatest(eval_defined_catch_binding_11075)
        false
    catch err
        err isa UndefVarError
    end
end

function generated_catch_binding_is_scoped_11075()
    try
        generated_catch_binding_11075(1)
        false
    catch err
        err isa UndefVarError
    end
end

@testset "eval tree-walker catch bindings remain scoped (Issue #11075)" begin
    @test eval_defined_catch_binding_is_scoped_11075()
    @test generated_catch_binding_is_scoped_11075()
end

true
