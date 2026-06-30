arr = [10, 20, 30, 40]
v = view(arr, 1:4)
r = reshape(v, 2, 2)

r[1, 1] == 10 &&
    r[2, 1] == 20 &&
    r[1, 2] == 30 &&
    r[2, 2] == 40 &&
    parent(r)[3] == 30 &&
    eltype(r) == Int64 &&
    size(r) == (2, 2) &&
    size(r, 3) == 1 &&
    length(r) == 4 &&
    ndims(r) == 2
