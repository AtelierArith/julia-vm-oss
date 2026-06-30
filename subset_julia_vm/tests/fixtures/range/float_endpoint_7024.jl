r = 0:0.1:0.3

length(r) == 4 &&
    r[1] == 0.0 &&
    r[2] == 0.1 &&
    r[3] == 0.2 &&
    abs(r[4] - 0.3) < 1.0e-12 &&
    abs(last(r) - 0.3) < 1.0e-12 &&
    length(collect(r)) == 4
