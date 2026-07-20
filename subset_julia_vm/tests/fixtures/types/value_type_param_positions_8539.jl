# Issue #8539: an integer value type parameter from a `where` clause must be
# usable in call-argument and range-endpoint positions.
struct Pos8539{N,T} end

fill_n(::Pos8539{N,T}) where {N,T} = fill(1, N)

function loop_sum(::Pos8539{N,T}) where {N,T}
    acc = 0
    for i in 1:N
        acc += i
    end
    return acc
end

range_collect(::Pos8539{N,T}) where {N,T} = collect(2:N)

vec_undef_len(::Pos8539{N,T}) where {N,T} = length(Vector{Float64}(undef, N))

two_params(::Pos8539{N,T}, ::Pos8539{M,S}) where {N,T,M,S} = fill(0.5, N + M)

function value_type_param_positions_8539()
    p = Pos8539{3,Float64}()
    q = Pos8539{2,Int}()

    ok_fill = fill_n(p) == [1, 1, 1]
    ok_loop = loop_sum(p) == 6
    ok_range = range_collect(p) == [2, 3]
    ok_undef = vec_undef_len(p) == 3
    ok_two = two_params(p, q) == fill(0.5, 5)

    return ok_fill && ok_loop && ok_range && ok_undef && ok_two
end

value_type_param_positions_8539()
