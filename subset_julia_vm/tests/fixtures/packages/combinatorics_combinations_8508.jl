using Combinatorics

function combinatorics_combinations_contract_8508()
    cs = combinations(1:4, 2)
    ok_length = length(cs) == 6

    values = []
    state = iterate(cs)
    while state !== nothing
        push!(values, state[1])
        state = iterate(cs, state[2])
    end

    ok_values = length(values) == 6 &&
        values[1] == [1, 2] &&
        values[2] == [1, 3] &&
        values[3] == [1, 4] &&
        values[4] == [2, 3] &&
        values[5] == [2, 4] &&
        values[6] == [3, 4]

    collected = collect(cs)
    ok_collect = collected == values

    enumerated = []
    for (i, c) in enumerate(cs)
        push!(enumerated, (i, c))
    end

    ok_enumerate = length(enumerated) == 6 &&
        enumerated[1] == (1, [1, 2]) &&
        enumerated[2] == (2, [1, 3]) &&
        enumerated[3] == (3, [1, 4]) &&
        enumerated[4] == (4, [2, 3]) &&
        enumerated[5] == (5, [2, 4]) &&
        enumerated[6] == (6, [3, 4])

    return ok_length && ok_values && ok_collect && ok_enumerate
end

combinatorics_combinations_contract_8508()
