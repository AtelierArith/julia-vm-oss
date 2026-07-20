struct SVector{N,T} <: StaticVector{N,T}
    data::Tuple
end

function SVector(x::AbstractVector)
    vals = []
    for i in 1:length(x)
        push!(vals, x[i])
    end
    return SVector(vals...)
end

function _svector_from_vector(::Type{T}, x::AbstractArray) where {T}
    n = length(x)
    if n == 0
        return SVector{0,T}(())
    elseif n == 1
        return SVector{1,T}((convert(T, x[1]),))
    elseif n == 2
        return SVector{2,T}((convert(T, x[1]), convert(T, x[2])))
    elseif n == 3
        return SVector{3,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3])))
    elseif n == 4
        return SVector{4,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4])))
    elseif n == 5
        return SVector{5,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5])))
    elseif n == 6
        return SVector{6,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5]), convert(T, x[6])))
    elseif n == 7
        return SVector{7,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5]), convert(T, x[6]), convert(T, x[7])))
    elseif n == 8
        return SVector{8,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5]), convert(T, x[6]), convert(T, x[7]), convert(T, x[8])))
    elseif n == 9
        return SVector{9,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5]), convert(T, x[6]), convert(T, x[7]), convert(T, x[8]), convert(T, x[9])))
    elseif n == 10
        return SVector{10,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5]), convert(T, x[6]), convert(T, x[7]), convert(T, x[8]), convert(T, x[9]), convert(T, x[10])))
    end
    error("SVector vector construction supports lengths 0 through 10")
end

function SVector(xs...)
    if length(xs) == 1 && xs[1] isa AbstractVector
        vals = []
        for i in 1:length(xs[1])
            push!(vals, xs[1][i])
        end
        return SVector(vals...)
    end
    return SVector{length(xs), typeof(xs[1])}(xs)
end

function SVector{N,T}(xs...) where {N,T}
    if length(xs) == 1 && xs[1] isa Tuple
        return SVector{N,T}(xs[1])
    elseif length(xs) == 1 && xs[1] isa AbstractVector
        return _svector_from_vector(T, xs[1])
    end
    return SVector{N,T}(xs)
end

function SVector{N}(xs...) where {N}
    # Single-tuple call `SVector{N}((a,b,...))` unwraps the flat tuple (Issue
    # #8084); otherwise the vararg method captures it as a single element.
    if length(xs) == 1 && xs[1] isa Tuple
        return SVector{N, typeof(xs[1][1])}(xs[1])
    elseif length(xs) == 1 && xs[1] isa AbstractVector
        return _svector_from_vector(eltype(xs[1]), xs[1])
    end
    return SVector{N, typeof(xs[1])}(xs)
end

function SVector{0,T}(x::AbstractVector) where {T}
    length(x) == 0 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{0,T}(())
end

function SVector{1,T}(x::AbstractVector) where {T}
    length(x) == 1 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{1,T}((convert(T, x[1]),))
end

function SVector{2,T}(x::AbstractVector) where {T}
    length(x) == 2 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{2,T}((convert(T, x[1]), convert(T, x[2])))
end

function SVector{3,T}(x::AbstractVector) where {T}
    length(x) == 3 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{3,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3])))
end

function SVector{4,T}(x::AbstractVector) where {T}
    length(x) == 4 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{4,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4])))
end

function SVector{5,T}(x::AbstractVector) where {T}
    length(x) == 5 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{5,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5])))
end

function SVector{6,T}(x::AbstractVector) where {T}
    length(x) == 6 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{6,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5]), convert(T, x[6])))
end

function SVector{7,T}(x::AbstractVector) where {T}
    length(x) == 7 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{7,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5]), convert(T, x[6]), convert(T, x[7])))
end

function SVector{8,T}(x::AbstractVector) where {T}
    length(x) == 8 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{8,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5]), convert(T, x[6]), convert(T, x[7]), convert(T, x[8])))
end

function SVector{9,T}(x::AbstractVector) where {T}
    length(x) == 9 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{9,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5]), convert(T, x[6]), convert(T, x[7]), convert(T, x[8]), convert(T, x[9])))
end

function SVector{10,T}(x::AbstractVector) where {T}
    length(x) == 10 || throw(DimensionMismatch("SVector length mismatch"))
    return SVector{10,T}((convert(T, x[1]), convert(T, x[2]), convert(T, x[3]), convert(T, x[4]), convert(T, x[5]), convert(T, x[6]), convert(T, x[7]), convert(T, x[8]), convert(T, x[9]), convert(T, x[10])))
end

macro SVector(ex)
    if ex isa Expr && ex.head == :vect
        return Expr(:call, :SVector, ex.args...)
    end
    error("@SVector currently supports literal vector expressions only (Issue #7459)")
end
