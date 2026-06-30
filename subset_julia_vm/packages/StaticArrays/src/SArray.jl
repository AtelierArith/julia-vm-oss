struct SArray{S,T,N,L} <: StaticArray{S,T,N}
    data::Tuple
end

function _static_shape_tuple(::Type{T}) where {T<:SArray}
    return tuple(T.parameters[1].parameters...)
end

function _static_length(::Type{T}) where {T<:SArray}
    return T.parameters[4]
end

function _static_eltype(::Type{T}) where {T<:SArray}
    return T.parameters[2]
end

function _static_ndims(::Type{T}) where {T<:SArray}
    return T.parameters[3]
end

function _static_matrix_literal_parts(ex)
    if ex isa Expr && ex.head == :hcat
        return (1, length(ex.args), ex.args)
    end
    if !(ex isa Expr && ex.head == :vcat)
        return nothing
    end

    rows = ex.args
    m = length(rows)
    if m == 0
        return nothing
    end

    first_row = rows[1]
    n = first_row isa Expr && first_row.head == :row ? length(first_row.args) : 1
    rowmajor = []
    for row in rows
        if row isa Expr && row.head == :row
            if length(row.args) != n
                error("matrix literal rows must have the same length")
            end
            for value in row.args
                push!(rowmajor, value)
            end
        else
            if n != 1
                error("matrix literal rows must have the same length")
            end
            push!(rowmajor, row)
        end
    end
    # Reorder the row-major source elements into column-major storage order to
    # match upstream StaticArrays / Julia (Issue #8084): the SMatrix backing
    # tuple lists column 1 top-to-bottom, then column 2, etc.
    values = []
    for j in 1:n
        for i in 1:m
            push!(values, rowmajor[(i - 1) * n + j])
        end
    end
    return (m, n, values)
end

macro SArray(ex)
    if ex isa Expr && ex.head == :vect
        return Expr(:call, :SVector, ex.args...)
    end
    parts = _static_matrix_literal_parts(ex)
    if parts !== nothing
        return Expr(:call, Expr(:curly, :SMatrix, parts[1], parts[2]), parts[3]...)
    end
    error("@SArray currently supports literal vector and matrix expressions (Issue #7733)")
end
