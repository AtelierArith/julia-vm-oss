# Issue #6225: nested vector literals must retain Vector{T} as their element
# type so runtime dispatch from an imprecise Any slot can still select the
# Vector{Vector{T}} method.

using Test

nested_vector_runtime_dispatch_6225(::Vector{T}) where {T} = :outer
nested_vector_runtime_dispatch_6225(::Vector{Vector{T}}) where {T} = :nested

function nested_vector_runtime_dispatch_via_any_6225(x)
    y::Any = x
    nested_vector_runtime_dispatch_6225(y)
end

@testset "nested Vector runtime dispatch from Any (Issue #6225)" begin
    xs = [[1], [2]]

    @test typeof(xs) === Vector{Vector{Int64}}
    @test nested_vector_runtime_dispatch_6225(xs) === :nested
    @test nested_vector_runtime_dispatch_via_any_6225(xs) === :nested
    @test nested_vector_runtime_dispatch_via_any_6225([[1], [2]]) === :nested
end

xs_6225 = [[1], [2]]
typeof(xs_6225) === Vector{Vector{Int64}} &&
    nested_vector_runtime_dispatch_6225(xs_6225) === :nested &&
    nested_vector_runtime_dispatch_via_any_6225(xs_6225) === :nested &&
    nested_vector_runtime_dispatch_via_any_6225([[1], [2]]) === :nested
