macro nothing_macro()
end
@test @expand(@nothing_macro) === nothing

let
    @test splitarg(:x) == (:x, :Any, false, nothing)
    @test splitarg(:(x=1)) == (:x, :Any, false, 1)
    @test splitarg(:(::Int)) == (nothing, :Int, false, nothing)
    @test combinearg(:x, :Any, false, nothing) == :(x::Any)
    @test combinearg(:x, :Any, true, nothing) == :(x...)

    dict = Dict(:name => :foo, :args => [:x], :kwargs => [], :body => :(x + 2))
    @test MacroTools.combinedef(dict).head == :function

    # Workaround: full @splitcombine support needs macro-generated
    # Expr(:function, ...) definitions to lower back to function definitions.
    # (Issue #7634)
end
