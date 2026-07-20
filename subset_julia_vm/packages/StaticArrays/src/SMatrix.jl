# `SMatrix{M,N,T,L}` mirrors upstream StaticArrays' canonical four-parameter
# form (`SMatrix{S1,S2,T,L} = SArray{Tuple{S1,S2},T,2,L}`), with `L` the flat
# backing-tuple length, always equal to `M*N` (Issue #11432). The bundled
# package keeps `SMatrix` as its own struct (rather than an `SArray` alias,
# Issue #7458), so `L` is validated by `check_array_parameters` (see
# abstractarray.jl) rather than inherited from `SArray`'s inner constructor.
#
# Upstream keeps the 3-parameter (and narrower) spellings constructible via
# incomplete parameterization: `SMatrix{M,N,T}` is `SMatrix{M,N,T,L} where L`
# (a `UnionAll` with `L` free), so a field annotation, `convert` target, or
# constructor call may drop `L` (or `T`, or `N`) and let it be inferred. sjulia
# supports this generically once `L` is a declared struct parameter — the
# `where {M,N,T}` methods below need no `L` themselves.
struct SMatrix{M,N,T,L} <: StaticMatrix{M,N,T}
    data::Tuple
end

function SMatrix(xs...)
    n = length(xs)
    return SMatrix{1, n, typeof(xs[1]), n}(xs)
end

function SMatrix{M,N,T,L}(xs...) where {M,N,T,L}
    check_array_parameters((M, N), 2, L)
    # Single-tuple call `SMatrix{M,N,T,L}((a,b,...))` unwraps the flat tuple, stored
    # column-major like upstream StaticArrays (Issue #8084). A typed single-arg
    # method would be cleaner but sjulia dispatch prefers this vararg form over a
    # `(x::Tuple)` method, so the unwrap lives here.
    if length(xs) == 1 && xs[1] isa Tuple
        return SMatrix{M,N,T,L}(xs[1])
    end
    return SMatrix{M,N,T,L}(xs)
end

function SMatrix{M,N,T}(xs...) where {M,N,T}
    # `L` is always inferrable from the flat argument count (Issue #11432);
    # `SMatrix{M,N,T,L}`'s own `check_array_parameters` call rejects a length
    # that does not match `M*N`, mirroring upstream's `DimensionMismatch`.
    # Workaround: (Issue #11539) pass `xs`/`xs[1]` as a single Tuple argument
    # rather than re-splatting `xs...` forward — splatting a vararg collection
    # into a runtime type-application curly whose trailing slot is a value
    # expression fails to resolve the expression.
    if length(xs) == 1 && xs[1] isa Tuple
        return SMatrix{M,N,T,length(xs[1])}(xs[1])
    end
    return SMatrix{M,N,T,length(xs)}(xs)
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
