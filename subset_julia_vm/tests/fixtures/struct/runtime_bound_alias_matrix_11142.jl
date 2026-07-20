module AliasBounds11142

const Exact = Union{Integer,String}
const Unique = Real

struct ExactBox{T<:AliasBounds11142.Exact}
    value::T
end

struct UniqueBox{T<:Unique}
    value::T
end

end

# Explicit inner constructors with alias-spelled where bounds are tracked
# separately by Issue #11003. These default constructors isolate the runtime
# struct-schema validation owned by Issue #11142.
runtime_exact_11142(::Type{T}, value) where T = AliasBounds11142.ExactBox{T}(value)
runtime_unique_11142(::Type{T}, value) where T = AliasBounds11142.UniqueBox{T}(value)

exact_apply = Core.apply_type(AliasBounds11142.ExactBox, Int)(1)
unique_apply = Core.apply_type(AliasBounds11142.UniqueBox, Float64)(2.5)
exact_new = runtime_exact_11142(String, "ok")
unique_new = runtime_unique_11142(Int, 3)

typeof(exact_apply) == AliasBounds11142.ExactBox{Int64} &&
    typeof(unique_apply) == AliasBounds11142.UniqueBox{Float64} &&
    typeof(exact_new) == AliasBounds11142.ExactBox{String} &&
    typeof(unique_new) == AliasBounds11142.UniqueBox{Int64} &&
    exact_apply.value == 1 &&
    unique_apply.value == 2.5 &&
    exact_new.value == "ok" &&
    unique_new.value == 3 &&
    true
