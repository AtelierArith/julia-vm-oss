struct SMatrix{M,N,T} <: StaticMatrix{M,N,T}
    data::Tuple
end

function SMatrix(xs...)
    return SMatrix{1, length(xs), typeof(xs[1])}(xs)
end

function SMatrix{M,N,T}(xs...) where {M,N,T}
    # Single-tuple call `SMatrix{M,N,T}((a,b,...))` unwraps the flat tuple, stored
    # column-major like upstream StaticArrays (Issue #8084). A typed single-arg
    # method would be cleaner but sjulia dispatch prefers this vararg form over a
    # `(x::Tuple)` method, so the unwrap lives here.
    if length(xs) == 1 && xs[1] isa Tuple
        return SMatrix{M,N,T}(xs[1])
    end
    return SMatrix{M,N,T}(xs)
end

function SMatrix{M,N}(xs...) where {M,N}
    if length(xs) == 1 && xs[1] isa Tuple
        return SMatrix{M,N, typeof(xs[1][1])}(xs[1])
    end
    return SMatrix{M,N, typeof(xs[1])}(xs)
end

macro SMatrix(ex)
    parts = _static_matrix_literal_parts(ex)
    if parts !== nothing
        return Expr(:call, Expr(:curly, :SMatrix, parts[1], parts[2]), parts[3]...)
    end
    error("@SMatrix currently supports literal matrix expressions (Issue #7733)")
end
