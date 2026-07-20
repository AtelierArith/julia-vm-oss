using Test

# Issue #10606: Union-kind type values have only the upstream fields `a` and
# `b`. DataType/UnionAll/TypeVar reflection fields must not leak through.

function is_union_field_error_10606(err, field)
    return err isa FieldError && err.type === Union && err.field === field
end

function dynamic_field_is_union_field_error_10606(u, field)
    try
        getfield(u, field)
        return false
    catch err
        return is_union_field_error_10606(err, field)
    end
end

function static_name_is_union_field_error_10606(u)
    try
        u.name
        return false
    catch err
        return is_union_field_error_10606(err, :name)
    end
end

function static_parameters_is_union_field_error_10606(u)
    try
        u.parameters
        return false
    catch err
        return is_union_field_error_10606(err, :parameters)
    end
end

@testset "Union reflection fields (Issue #10606)" begin
    u = Union{Int64, Float64}

    @test fieldnames(Union) == (:a, :b)
    @test ((u.a === Int64 && u.b === Float64) || (u.a === Float64 && u.b === Int64))

    for field in (:name, :parameters, :var, :body, :lb, :ub, :bogus)
        @test dynamic_field_is_union_field_error_10606(u, field)
    end

    @test static_name_is_union_field_error_10606(u)
    @test static_parameters_is_union_field_error_10606(u)
end

true
