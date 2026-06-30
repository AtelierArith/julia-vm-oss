using Test

f4270_idx(a) = a[1] !== nothing ? a[1] : 0

@test Base.infer_return_type(f4270_idx, Tuple{Vector{Union{Int64,Nothing}}}) == Union{Nothing,Int64}
@test Core.Compiler.return_type(f4270_idx, Tuple{Vector{Union{Int64,Nothing}}}) == Union{Nothing,Int64}
@test f4270_idx([1, nothing]) == 1
@test f4270_idx([nothing, 1]) == 0

true
