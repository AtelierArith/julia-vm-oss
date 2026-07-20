# Issue #11539: splatting the SAME vararg collection forward into a runtime
# type-application curly (`Foo{M,N,T,n}(xs...)`) must resolve the trailing
# runtime value-parameter (a caller `where`-bound type variable, or an inline
# call expression such as `length(xs)`) identically to the non-splat form
# (`Foo{M,N,T,n}(xs)`). Previously the compiler's runtime type-parameter
# binding path mis-resolved the whole curly text as an undefined variable
# name (`UndefVarError: Foo{M, N, T, n} not defined`) whenever the call
# argument was a splat.

struct Foo{M,N,T,L}
    data::Tuple
end

function Foo{M,N,T,L}(xs...) where {M,N,T,L}
    return Foo{M,N,T,L}(xs)
end

# Local-variable trailing value parameter, splatted forward.
function Foo{M,N,T}(xs...) where {M,N,T}
    n = length(xs)
    return Foo{M,N,T,n}(xs...)
end

x = Foo{2,2,Float64}(1.0, 2.0, 3.0, 4.0)
println(typeof(x))
println(x.data)
@assert typeof(x) == Foo{2,2,Float64,4}
@assert x.data == (1.0, 2.0, 3.0, 4.0)

# Inline-call trailing value parameter, splatted forward (no intermediate
# local binding).
struct Bar{M,N,T,L}
    data::Tuple
end

function Bar{M,N,T,L}(xs...) where {M,N,T,L}
    return Bar{M,N,T,L}(xs)
end

function Bar{M,N,T}(xs...) where {M,N,T}
    return Bar{M,N,T,length(xs)}(xs...)
end

y = Bar{3,1,Int}(10, 20, 30)
println(typeof(y))
println(y.data)
@assert typeof(y) == Bar{3,1,Int,3}
@assert y.data == (10, 20, 30)

# The pre-existing non-splat forwarding form (passing the vararg tuple as a
# single positional argument) must keep working unchanged.
struct Baz{M,N,T,L}
    data::Tuple
end

function Baz{M,N,T,L}(xs...) where {M,N,T,L}
    return Baz{M,N,T,L}(xs)
end

function Baz{M,N,T}(xs...) where {M,N,T}
    n = length(xs)
    return Baz{M,N,T,n}(xs)
end

z = Baz{1,1,Float64}(9.0)
println(typeof(z))
println(z.data)
@assert typeof(z) == Baz{1,1,Float64,1}
@assert z.data == (9.0,)

# A module-qualified parametric struct's splat-forwarded constructor must
# resolve through the same runtime type-application path as the top-level
# case: `owned_constructor_name_in_scope` recognizes only the module-qualified
# form, whose target eagerly (and incorrectly) resolves type arguments at
# compile time, so this exercises a distinct routing branch from Foo/Bar/Baz
# above.
module SplatVarargForwardTypeApplyM11539
struct Qux{M,N,T,L}
    data::Tuple
end

function Qux{M,N,T,L}(xs...) where {M,N,T,L}
    return Qux{M,N,T,L}(xs)
end

function Qux{M,N,T}(xs...) where {M,N,T}
    n = length(xs)
    return Qux{M,N,T,n}(xs...)
end
end

w = SplatVarargForwardTypeApplyM11539.Qux{2,2,Float64}(1.0, 2.0, 3.0, 4.0)
println(typeof(w))
println(w.data)
@assert typeof(w) == SplatVarargForwardTypeApplyM11539.Qux{2,2,Float64,4}
@assert w.data == (1.0, 2.0, 3.0, 4.0)

true
