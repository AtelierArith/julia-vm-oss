struct SVector{N,T} <: StaticVector{N,T}
    data::Tuple
end

function SVector(xs...)
    return SVector{length(xs), typeof(xs[1])}(xs)
end

function SVector{N,T}(xs...) where {N,T}
    if length(xs) == 1 && xs[1] isa Tuple
        return SVector{N,T}(xs[1])
    end
    return SVector{N,T}(xs)
end

function SVector{N}(xs...) where {N}
    # Single-tuple call `SVector{N}((a,b,...))` unwraps the flat tuple (Issue
    # #8084); otherwise the vararg method captures it as a single element.
    if length(xs) == 1 && xs[1] isa Tuple
        return SVector{N, typeof(xs[1][1])}(xs[1])
    end
    return SVector{N, typeof(xs[1])}(xs)
end

macro SVector(ex)
    if ex isa Expr && ex.head == :vect
        return Expr(:call, :SVector, ex.args...)
    end
    error("@SVector currently supports literal vector expressions only (Issue #7459)")
end
