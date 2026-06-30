function arrays_index_assign_i64_loop_6346!(a, n)
    for i in 1:n
        a[i] = i * 3
    end
    return a[n]
end

function arrays_index_assign_f64_loop_6346!(a, n)
    for i in 1:n
        a[i] = Float64(i) * 0.5
    end
    return a[n]
end

ints = Vector{Int64}(undef, 5)
floats = Vector{Float64}(undef, 4)

arrays_index_assign_i64_loop_6346!(ints, 5) == 15 &&
    arrays_index_assign_f64_loop_6346!(floats, 4) == 2.0
