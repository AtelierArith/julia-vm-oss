function board(id::AbstractString="box"; xlim=(-5, 5), ylim=(-5, 5), axis=true, grid=false,
               width=500, height=500, showNavigation=false, showCopyright=false, kwargs...)
    # Workaround: explicit Float64[] avoids Memory{Int64} cache collision (W-39) when board()
    # is first called with integer defaults then Float64 coords (Issue #8072).
    bb = Float64[xlim[1], ylim[2], xlim[2], ylim[1]]
    opts = Pair{Symbol,Any}[:boundingbox=>bb, :axis=>axis, :grid=>grid,
                            :width=>width, :height=>height,
                            :showNavigation=>showNavigation, :showCopyright=>showCopyright]
    _append_kwargs!(opts, kwargs)
    return Board(Any[], opts)
end

function board(f::Function, id::AbstractString="box"; kwargs...)
    b = board(id; kwargs...)
    f(b)
    return b
end

function Base.push!(b::Board, elems...)
    for e in elems
        push!(b.elements, e)
    end
    return b
end

function view3d(position, size, ranges; kwargs...)
    return View3D(_new_id(), Any[position, size, ranges], _kw_to_attrs(kwargs), Any[])
end

function view3d(f::Function, position, size, ranges; kwargs...)
    v = view3d(position, size, ranges; kwargs...)
    f(v)
    return v
end

function Base.push!(v::View3D, elems...)
    for e in elems
        push!(v.elements, e)
    end
    return v
end

html(b::Board) = b
