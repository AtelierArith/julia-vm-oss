xs = [1.0, 2.0, 3.0]
ys = xs .+ 2.0
ys[1] = 99.0

mat = reshape([1.0, 2.0, 3.0, 4.0], 2, 2)
shifted = mat .+ [10.0, 20.0]

xs[1] == 1.0 &&
    ys == [99.0, 4.0, 5.0] &&
    size(shifted) == (2, 2) &&
    shifted[1, 1] == 11.0 &&
    shifted[2, 1] == 22.0 &&
    shifted[1, 2] == 13.0 &&
    shifted[2, 2] == 24.0
