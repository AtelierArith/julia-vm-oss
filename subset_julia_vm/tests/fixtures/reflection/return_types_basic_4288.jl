using Test

function reflection_ret4288(x::Int64)
    x + 1
end

reflection_tuple_ret4288(x::Tuple{Int64,Float64}) = (x[1], x[2])
reflection_array_ret4288(xs::Vector{Int64}) = xs
reflection_map_ret4288(xs::Vector{Int64}) = map(x -> x + 1, xs)
reflection_tuple_map_ret4288(t::Tuple{Int64,Float64}) = map(x -> x + 1, t)
reflection_generator_ret4288(xs::Vector{Int64}) = collect(x + 1 for x in xs)
reflection_dispatch_ret4288(x::Integer) = 1
reflection_dispatch_ret4288(x::Number) = 1.0
reflection_type_ret4288(::Type{T}) where {T} = T
reflection_untyped_arg_ret4296(x) = x + 1
reflection_union_direct_ret4288(x::Int64) = 1
reflection_union_direct_ret4288(x::String) = "s"
reflection_union_callee_ret4288(x::Int64) = 1
reflection_union_callee_ret4288(x::Float64) = 2.0
reflection_union_caller_ret4288(x::Union{Int64,Float64}) = reflection_union_callee_ret4288(x)
reflection_union_ternary_ret4288(x::Union{Int64,String}) = x isa Int64 ? x + 1 : length(x)

@testset "reflection return_types and infer_return_type basic" begin
    rts_tuple = Base.return_types(reflection_ret4288, Tuple{Int64})
    @test length(rts_tuple) == 1
    @test rts_tuple[1] === Int64
    @test Base.infer_return_type(reflection_ret4288, Tuple{Int64}) === Int64

    rts_value_tuple = Base.return_types(reflection_ret4288, (Int64,))
    @test length(rts_value_tuple) == 1
    @test rts_value_tuple[1] === Int64
    @test Base.infer_return_type(reflection_ret4288, (Int64,)) === Int64
    @test Core.Compiler.return_type(reflection_ret4288, Tuple{Int64}) === Int64
    @test Core.Compiler.return_type(Tuple{typeof(reflection_ret4288), Int64}) === Int64
    @test Base.return_types(+, Tuple{Int64,Int64})[1] === Int64
    @test Base.infer_return_type(+, Tuple{Int64,Int64}) === Int64
    @test Core.Compiler.return_type(+, Tuple{Int64,Int64}) === Int64
    @test Base.return_types(abs, Tuple{Float64})[1] === Float64
    @test Base.infer_return_type(abs, Tuple{Float64}) === Float64
    @test Core.Compiler.return_type(abs, Tuple{Float64}) === Float64
    @test Base.return_types(string, Tuple{Int64})[1] === String
    @test Base.infer_return_type(string, Tuple{Int64}) === String
    @test Core.Compiler.return_type(string, Tuple{Int64}) === String
    @test Base.return_types(length, Tuple{Vector{Int64}})[1] === Int64
    @test Base.infer_return_type(length, Tuple{Vector{Int64}}) === Int64
    @test Base.return_types(getindex, Tuple{Vector{Int64},Int64})[1] === Int64
    @test Base.infer_return_type(getindex, Tuple{Vector{Int64},Int64}) === Int64

    rts_no_match = Base.return_types(reflection_ret4288, Tuple{String})
    @test length(rts_no_match) == 0
    @test Base.infer_return_type(reflection_ret4288, Tuple{String}) === Union{}

    rts_tuple_ret = Base.return_types(reflection_tuple_ret4288, Tuple{Tuple{Int64,Float64}})
    @test length(rts_tuple_ret) == 1
    @test rts_tuple_ret[1] == Tuple{Int64,Float64}

    rts_array_ret = Base.return_types(reflection_array_ret4288, Tuple{Vector{Int64}})
    @test length(rts_array_ret) == 1
    @test rts_array_ret[1] == Vector{Int64}
    @test Base.infer_return_type(reflection_array_ret4288, Tuple{Vector{Int64}}) == Vector{Int64}

    rts_map_ret = Base.return_types(reflection_map_ret4288, Tuple{Vector{Int64}})
    @test length(rts_map_ret) == 1
    @test rts_map_ret[1] == Vector{Int64}
    @test Base.infer_return_type(reflection_map_ret4288, Tuple{Vector{Int64}}) == Vector{Int64}

    rts_tuple_map_ret = Base.return_types(reflection_tuple_map_ret4288, Tuple{Tuple{Int64,Float64}})
    @test length(rts_tuple_map_ret) == 1
    @test rts_tuple_map_ret[1] == Tuple{Int64,Float64}
    @test Base.infer_return_type(reflection_tuple_map_ret4288, Tuple{Tuple{Int64,Float64}}) == Tuple{Int64,Float64}

    rts_generator_ret = Base.return_types(reflection_generator_ret4288, Tuple{Vector{Int64}})
    @test length(rts_generator_ret) == 1
    @test rts_generator_ret[1] == Vector{Int64}
    @test Base.infer_return_type(reflection_generator_ret4288, Tuple{Vector{Int64}}) == Vector{Int64}

    rts_dispatch_int = Base.return_types(reflection_dispatch_ret4288, Tuple{Int64})
    @test length(rts_dispatch_int) == 1
    @test rts_dispatch_int[1] === Int64
    @test Base.infer_return_type(reflection_dispatch_ret4288, Tuple{Int64}) === Int64

    rts_dispatch_float = Base.return_types(reflection_dispatch_ret4288, Tuple{Float64})
    @test length(rts_dispatch_float) == 1
    @test rts_dispatch_float[1] === Float64
    @test Base.infer_return_type(reflection_dispatch_ret4288, Tuple{Float64}) === Float64

    rts_type_ret = Base.return_types(reflection_type_ret4288, Tuple{Type{Int64}})
    @test length(rts_type_ret) == 1
    @test rts_type_ret[1] == Type{Int64}
    @test Base.infer_return_type(reflection_type_ret4288, Tuple{Type{Int64}}) == Type{Int64}

    rts_untyped_arg_ret = Base.return_types(reflection_untyped_arg_ret4296, Tuple{Int64})
    @test length(rts_untyped_arg_ret) == 1
    @test rts_untyped_arg_ret[1] === Int64
    @test Base.infer_return_type(reflection_untyped_arg_ret4296, Tuple{Int64}) === Int64

    rts_union_direct = Base.return_types(reflection_union_direct_ret4288, Tuple{Union{Int64,String}})
    @test length(rts_union_direct) == 2
    @test rts_union_direct[1] === Int64 || rts_union_direct[2] === Int64
    @test rts_union_direct[1] === String || rts_union_direct[2] === String
    @test Base.infer_return_type(reflection_union_direct_ret4288, Tuple{Union{Int64,String}}) == Union{Int64,String}
    @test Core.Compiler.return_type(reflection_union_direct_ret4288, Tuple{Union{Int64,String}}) == Union{Int64,String}

    rts_union_caller = Base.return_types(reflection_union_caller_ret4288, Tuple{Union{Int64,Float64}})
    @test length(rts_union_caller) == 1
    @test rts_union_caller[1] == Union{Int64,Float64}
    @test string(Base.infer_return_type(reflection_union_caller_ret4288, Tuple{Union{Int64,Float64}})) == "Union{Float64, Int64}"

    rts_union_ternary = Base.return_types(reflection_union_ternary_ret4288, Tuple{Union{Int64,String}})
    @test length(rts_union_ternary) == 1
    @test rts_union_ternary[1] === Int64
    @test Base.infer_return_type(reflection_union_ternary_ret4288, Tuple{Union{Int64,String}}) === Int64
end

true
