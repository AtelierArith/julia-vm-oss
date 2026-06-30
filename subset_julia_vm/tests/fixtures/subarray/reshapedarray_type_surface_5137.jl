arr = [10, 20, 30, 40]
v = view(arr, 1:4)
r = reshape(v, 2, 2)

@assert occursin("ReshapedArray{Int64, 2", string(typeof(r)))
@assert occursin("SubArray{Int64, 1", string(typeof(r)))
@assert r isa AbstractArray{Int64,2}
@assert r isa AbstractMatrix{Int64}
@assert parent(r) === v
@assert r[2, 1] == 20

r[1, 2] = 99
@assert arr[3] == 99

arr[4] = 77
@assert r[2, 2] == 77

arr2 = [1, 2, 3, 4]
mat = reshape(arr2, 2, 2)
@assert typeof(mat) === Matrix{Int64}
@assert mat[2, 2] == 4

true
