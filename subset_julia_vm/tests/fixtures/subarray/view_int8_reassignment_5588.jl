arr = [4, 5, 6]
v = view(arr, 1:2)
int64_view_ok = eltype(v) == Int64 && collect(v) == [4, 5]

arr = Int8[4, 5, 6]
v = view(arr, 1:2)
int8_view_ok = eltype(v) == Int8 && collect(v) == Int8[4, 5] && v[1] == Int8(4)

int64_view_ok && int8_view_ok
