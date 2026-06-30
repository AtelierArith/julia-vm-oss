function inbounds_tuple_destructuring_contract_8405()
    xs = [1.0, 3.0]
    ys = [2.0, 4.0]

    vals = map(1:2) do i
        @inbounds a, b = xs[i], ys[i]
        a + b
    end

    total = 0.0
    for i in eachindex(xs)
        @inbounds x, y = xs[i], ys[i]
        total += x * y
    end

    vals[1] == 3.0 && vals[2] == 7.0 && total == 14.0
end

inbounds_tuple_destructuring_contract_8405()
