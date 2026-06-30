using Test

struct PairBox4269
    a
    b
end

make_box4269(flag) = PairBox4269(flag ? 1 : 2, "x")
use_box_getfield4269(flag) = getfield(make_box4269(flag), :b)

function use_box_local4269(flag)
    box = make_box4269(flag)
    return getfield(box, :b)
end

function local_box4269(flag)
    box = PairBox4269(flag ? 1 : 2, "x")
    return getfield(box, :b)
end

# Issue #4849: immutable parametric default constructor — the type parameter
# `T` is bound from the field argument type, so the concrete parametric struct
# return and PartialStruct-style field facts must survive interprocedural calls.
struct PBoxParam4269{T}
    a::T
    b
end

make_param4269(flag) = PBoxParam4269(flag ? 1 : 2, "x")
use_param4269(flag) = getfield(make_param4269(flag), :b)

# Issue #4850: explicit parametric inner constructor using `new{T}(...)`. The
# call spells out the concrete type arguments (`{Int64}`); analyzing the inner
# constructor body recovers `ParamInner4269{Int64}` and the field facts.
struct ParamInner4269{T}
    a::T
    b
    function ParamInner4269{T}(x) where {T}
        new{T}(x, "x")
    end
end

make_param_inner4269(flag) = ParamInner4269{Int64}(flag ? 1 : 2)
use_param_inner4269(flag) = getfield(make_param_inner4269(flag), :b)

# Issue #4851: parametric default constructor whose type parameter is embedded
# inside a nested field type (`Tuple{T,T}`) rather than a bare `field::T`.
struct NestedParamBox4269{T}
    a::Tuple{T,T}
    b
end

make_nested_param4269(flag) = NestedParamBox4269((flag ? 1 : 2, 3), "x")
use_nested_param4269(flag) = getfield(make_nested_param4269(flag), :b)

@testset "PartialStruct return field inference (Issue #4269)" begin
    @test make_box4269(true) isa PairBox4269
    @test use_box_getfield4269(true) == "x"
    @test use_box_local4269(false) == "x"
    @test local_box4269(true) == "x"

    @test Base.infer_return_type(make_box4269, Tuple{Bool}) == PairBox4269
    @test Base.return_types(make_box4269, Tuple{Bool})[1] == PairBox4269
    @test Base.infer_return_type(use_box_getfield4269, Tuple{Bool}) == String
    @test Base.return_types(use_box_getfield4269, Tuple{Bool})[1] == String
    @test Base.infer_return_type(use_box_local4269, Tuple{Bool}) == String
    @test Base.return_types(use_box_local4269, Tuple{Bool})[1] == String
    @test Base.infer_return_type(local_box4269, Tuple{Bool}) == String
    @test Base.return_types(local_box4269, Tuple{Bool})[1] == String
end

@testset "Parametric immutable default constructor inference (Issue #4849)" begin
    @test make_param4269(true) isa PBoxParam4269{Int64}
    @test use_param4269(true) == "x"

    @test Base.infer_return_type(make_param4269, Tuple{Bool}) == PBoxParam4269{Int64}
    @test Base.return_types(make_param4269, Tuple{Bool})[1] == PBoxParam4269{Int64}
    @test Base.infer_return_type(use_param4269, Tuple{Bool}) == String
    @test Base.return_types(use_param4269, Tuple{Bool})[1] == String
end

@testset "Explicit parametric inner constructor inference (Issue #4850)" begin
    @test make_param_inner4269(true) isa ParamInner4269{Int64}
    @test use_param_inner4269(true) == "x"

    @test Base.infer_return_type(make_param_inner4269, Tuple{Bool}) == ParamInner4269{Int64}
    @test Base.return_types(make_param_inner4269, Tuple{Bool})[1] == ParamInner4269{Int64}
    @test Base.infer_return_type(use_param_inner4269, Tuple{Bool}) == String
    @test Base.return_types(use_param_inner4269, Tuple{Bool})[1] == String
end

@testset "Nested tuple field parametric constructor inference (Issue #4851)" begin
    @test make_nested_param4269(true) isa NestedParamBox4269{Int64}
    @test use_nested_param4269(true) == "x"

    @test Base.infer_return_type(make_nested_param4269, Tuple{Bool}) == NestedParamBox4269{Int64}
    @test Base.return_types(make_nested_param4269, Tuple{Bool})[1] == NestedParamBox4269{Int64}
    @test Base.infer_return_type(use_nested_param4269, Tuple{Bool}) == String
    @test Base.return_types(use_nested_param4269, Tuple{Bool})[1] == String
end

true
