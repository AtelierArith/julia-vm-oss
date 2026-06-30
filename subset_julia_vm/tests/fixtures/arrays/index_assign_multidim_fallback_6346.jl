function arrays_index_assign_matrix_fallback_6346!(a)
    a[1, 2] = 7
    return a[1, 2]
end

matrix = zeros(Int64, 2, 2)
arrays_index_assign_matrix_fallback_6346!(matrix) == 7
