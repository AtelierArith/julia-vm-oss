empty_find = findall(x -> x > 0, Float64[])
nonempty_find = findall(x -> x > 2, [1.0, 2.0, 3.0, 4.0])
array_count = count(x -> x > 2, [1, 2, 3, 4, 5])
right_fold = mapfoldr(x -> x + 1, -, [1.0, 2.0, 3.0])
tuple_result = ntuple(i -> i * 2, 3)
float32_count = count(x -> x > 1.0f0, Float32[0.5f0, 2.0f0, 3.0f0])

length(empty_find) == 0 &&
    nonempty_find == [3, 4] &&
    array_count == 3 &&
    right_fold == 3.0 &&
    tuple_result == (2.0, 4.0, 6.0) &&
    float32_count == 2
