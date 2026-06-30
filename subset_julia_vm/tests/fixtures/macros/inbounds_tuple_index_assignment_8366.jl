function inbounds_tuple_index_assignment_contract_8366()
    x = [-0.7745966692414834, 0.0, 0.0]
    w = Any[0.5555555555555556, 0.8888888888888888, 0.0]
    n = 2
    n_total = 3

    for i = n + 1:n_total
        j = 2n + iseven(n_total) - i
        @inbounds x[i], w[i] = -x[j], w[j]
    end

    x[1] == -0.7745966692414834 &&
        x[2] == 0.0 &&
        x[3] == 0.7745966692414834 &&
        w[1] == 0.5555555555555556 &&
        w[2] == 0.8888888888888888 &&
        w[3] == 0.5555555555555556
end

inbounds_tuple_index_assignment_contract_8366()
