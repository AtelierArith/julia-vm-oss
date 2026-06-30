function array_any_local_boundary_4021()
    x = 0
    x = [1, 2, 3]
    push!(x, 4)
    return length(x) == 4 && x[4] == 4
end

array_any_local_boundary_4021()

function array_any_parameter_boundary_4021(a)
    push!(a, 5)
    return length(a) == 5 && a[5] == 5
end

array_any_parameter_boundary_4021([1, 2, 3, 4])
