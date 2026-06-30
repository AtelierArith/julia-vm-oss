import Base: collect

collect(a::Array{Int64}) = :array_override
collect(a::Array{Real}) = :real_array_override
collect(a::Array{T}) where T = :array_t_override

runtime_collect_array_unionall_4581(x::Any) = collect(x)

@assert runtime_collect_array_unionall_4581([1, 2, 3]) === :array_override
@assert runtime_collect_array_unionall_4581(Float64[1.0, 2.0]) === :array_t_override

true
