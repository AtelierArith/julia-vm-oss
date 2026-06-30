using LinearAlgebra

H = SymTridiagonal([0.0, 0.0, 0.0], [0.5773502691896258, 0.5163977794943222])
M = Matrix(H)
vals = eigvals(H, 1:2)

M isa Matrix &&
    size(M) == (3, 3) &&
    M[1, 1] == 0.0 &&
    M[1, 2] == 0.5773502691896258 &&
    M[2, 1] == 0.5773502691896258 &&
    length(vals) == 2 &&
    vals[1] < vals[2]
