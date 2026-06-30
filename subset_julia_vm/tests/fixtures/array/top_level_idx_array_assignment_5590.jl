idx = [1, 2, 3]
check_literal = idx == [1, 2, 3] &&
    typeof(idx) == Vector{Int64} &&
    length(idx) == 3 &&
    idx[1] == 1 &&
    idx[end] == 3

filtered = [true, false, true]
idx = findall(filtered)
check_findall = idx == [1, 3] &&
    typeof(idx) == Vector{Int64} &&
    length(idx) == 2 &&
    idx[1] == 1 &&
    idx[end] == 3

check_literal && check_findall
