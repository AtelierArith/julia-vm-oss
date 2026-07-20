abstract type NarrowMatrix11230 <: AbstractMatrix{Float64} end
abstract type NarrowVector11230 <: AbstractVector{Float64} end

struct Matrix11230 <: NarrowMatrix11230 end
struct Vector11230 <: NarrowVector11230 end

Base.:*(::AbstractMatrix, ::AbstractVector) = "generic"
Base.:*(::NarrowMatrix11230, ::NarrowVector11230) = "specific"

result = Matrix11230() * Vector11230()
println(result)
result == "specific" || error("narrow binary method lost to broad array method")
true
