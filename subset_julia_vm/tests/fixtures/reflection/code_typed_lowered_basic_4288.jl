using Test

reflection_code4288(x::Int64) = x + 1
reflection_code_map4288(xs::Vector{Int64}) = map(x -> x + 1, xs)
reflection_code_tuple_map4288(t::Tuple{Int64,Float64}) = map(x -> x + 1, t)
reflection_code_generator4288(xs::Vector{Int64}) = collect(x + 1 for x in xs)

function reflection_code_contains_expr_4288(code)
    for item in code
        if item isa Expr
            return true
        end
    end
    false
end

@testset "reflection code_lowered/code_typed basic (Issue #4288)" begin
    lowered = Base.code_lowered(reflection_code4288, Tuple{Int64})
    typed = Base.code_typed(reflection_code4288, Tuple{Int64})
    typed_noopt = Base.code_typed(reflection_code4288, Tuple{Int64}; optimize=false)
    typed_debuginfo = Base.code_typed(reflection_code4288, Tuple{Int64}; debuginfo=:source)
    typed_debuginfo_none = Base.code_typed(reflection_code4288, Tuple{Int64}; debuginfo=:none)
    typed_interp_nothing = Base.code_typed(reflection_code4288, Tuple{Int64}; interp=nothing)
    lowered_debuginfo_none = Base.code_lowered(reflection_code4288, Tuple{Int64}; debuginfo=:none)
    typed_map = Base.code_typed(reflection_code_map4288, Tuple{Vector{Int64}})
    typed_tuple_map = Base.code_typed(reflection_code_tuple_map4288, Tuple{Tuple{Int64,Float64}})
    typed_generator = Base.code_typed(reflection_code_generator4288, Tuple{Vector{Int64}})
    code_typed_generated_rejected_4288 = false
    try
        Base.code_typed(reflection_code4288, Tuple{Int64}; generated=false)
    catch err
        code_typed_generated_rejected_4288 = true
    end

    @test length(lowered) == 1
    @test lowered[1] !== nothing
    @test length(typed) == 1
    @test typed[1][1] !== nothing
    @test typed[1][2] === Int64
    @test typed[1][1].rettype === Int64
    @test hasfield(typeof(typed[1][1]), :code)
    @test length(typed[1][1].code) > 0
    @test reflection_code_contains_expr_4288(typed[1][1].code)
    @test hasfield(typeof(lowered[1]), :code)
    @test length(lowered[1].code) > 0
    @test reflection_code_contains_expr_4288(lowered[1].code)
    @test typed_noopt[1][2] === Int64
    @test typed_debuginfo[1][2] === Int64
    @test typed_debuginfo_none[1][2] === Int64
    @test typed_interp_nothing[1][2] === Int64
    @test length(lowered_debuginfo_none) == 1
    @test_throws ArgumentError Base.code_typed(reflection_code4288, Tuple{Int64}; debuginfo=:bogus)
    @test_throws ArgumentError Base.code_lowered(reflection_code4288, Tuple{Int64}; debuginfo=:bogus)
    @test_throws ErrorException Base.code_typed(reflection_code4288, Tuple{Int64}; interp=1)
    @test code_typed_generated_rejected_4288
    @test length(typed_map) == 1
    @test typed_map[1][1] !== nothing
    @test typed_map[1][2] == Vector{Int64}
    @test length(typed_tuple_map) == 1
    @test typed_tuple_map[1][1] !== nothing
    @test typed_tuple_map[1][2] == Tuple{Int64,Float64}
    @test length(typed_generator) == 1
    @test typed_generator[1][1] !== nothing
    @test typed_generator[1][2] == Vector{Int64}
end

true
