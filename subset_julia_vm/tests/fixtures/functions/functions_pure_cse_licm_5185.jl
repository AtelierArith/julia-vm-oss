using Test

function cse_licm(xs)
    total = 0
    for i in 1:5
        total += length(xs)
    end

    first_len = length(xs)
    second_len = length(xs)
    return total + first_len + second_len
end

xs = [1, 2, 3, 4]
@test cse_licm(xs) == 28

true
