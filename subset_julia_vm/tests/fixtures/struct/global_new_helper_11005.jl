using Test

# Issue #11005: a struct body may define a differently named GLOBAL helper whose
# body calls `new` — this is how upstream `Rational` (julia/base/rational.jl)
# exposes its unchecked terminal constructor `unsafe_rational`. The helper is an
# ordinary global method; only its body keeps the struct body's privileged `new`.

struct RawCtorProbe11005{T}
    x::T
    global raw_ctor_probe11005(::Type{T}, x) where {T} = new{T}(x)
end

r11005 = raw_ctor_probe11005(Int, 7)
@test typeof(r11005) === RawCtorProbe11005{Int}
@test r11005.x == 7
@test typeof(raw_ctor_probe11005(Float64, 1.5)) === RawCtorProbe11005{Float64}

# Non-parametric struct: the helper bypasses the checking inner constructor.
struct Plain11005
    x::Int
    Plain11005(x) = new(x + 1)
    global raw_plain11005(x) = new(x)
end

@test Plain11005(5).x == 6
@test raw_plain11005(5).x == 5

# Long-form `global function ... end` helper.
struct LongHelper11005{T}
    v::T
    global function long_helper11005(::Type{T}, v) where {T}
        new{T}(v)
    end
end

@test typeof(long_helper11005(Float64, 1.5)) === LongHelper11005{Float64}
@test long_helper11005(Float64, 1.5).v == 1.5

# The struct body's privileged `new` is lexical and therefore survives inside
# a closure nested in the global helper. This path requires the live
# LambdaContext so the lifted function retains the same constructor owner
# (Issue #11179).
struct LiftedHelper11179{T}
    v::T
    global lifted_helper11179(::Type{T}, v) where {T} = (() -> new{T}(v))()
end

lifted11179 = lifted_helper11179(Int, 11)
@test typeof(lifted11179) === LiftedHelper11179{Int}
@test lifted11179.v == 11

# Ordinary nested functions remain in the helper's lexical constructor scope,
# while runtime @eval deliberately starts a Main-global scope where `new` is an
# ordinary undefined binding. Constructor authority must stop at that boundary
# instead of leaking into the eval-defined function (Issue #11197).
struct EvalBoundary11197
    v
    global function eval_boundary11197(v)
        lexical11197() = new(v)
        @eval leaked_new11197() = new(99)
        @eval leaked_lambda11204() = (() -> new(98))()
        @eval leaked_task11197() = fetch(@async new(97))
        lexical11197()
    end
end
eval_boundary_value11197 = eval_boundary11197(16)
@test typeof(eval_boundary_value11197) === EvalBoundary11197
@test eval_boundary_value11197.v == 16
@test_throws UndefVarError leaked_new11197()
@test_throws UndefVarError leaked_lambda11204()
@test_throws TaskFailedException leaked_task11197()

# Transparent top-level blocks and @kwdef must register the helper methods too;
# retaining the StructDef while dropping its global_new_helpers field leaves an
# Unknown function at the call site (Issue #11186).
begin
    struct BeginHelper11186{T}
        v::T
        global begin_helper11186(::Type{T}, v) where {T} = (() -> new{T}(v))()
    end
end
begin11186 = begin_helper11186(Int, 12)
@test typeof(begin11186) === BeginHelper11186{Int}
@test begin11186.v == 12

let
    struct LetHelper11186{T}
        v::T
        global let_helper11186(::Type{T}, v) where {T} = (() -> new{T}(v))()
    end
end
let11186 = let_helper11186(Int, 13)
@test typeof(let11186) === LetHelper11186{Int}
@test let11186.v == 13

rhs11186 = begin
    struct RhsHelper11186{T}
        v::T
        global rhs_helper11186(::Type{T}, v) where {T} = (() -> new{T}(v))()
    end
    nothing
end
@test rhs11186 === nothing
rhs_value11186 = rhs_helper11186(Int, 14)
@test typeof(rhs_value11186) === RhsHelper11186{Int}
@test rhs_value11186.v == 14

Base.@kwdef struct KwdefHelper11186{T}
    v::T
    global kwdef_helper11186(::Type{T}, v) where {T} = (() -> new{T}(v))()
end
kwdef11186 = kwdef_helper11186(Int, 15)
@test typeof(kwdef11186) === KwdefHelper11186{Int}
@test kwdef11186.v == 15

# Preserve every positional argument and every splat position. The old
# boolean-only normalization kept only the final splatted expression, silently
# constructing a malformed one-field value (Issues #11183/#11187).
struct MixedSplatNew11187
    a
    b
    c
    d
    global mixed_splat_new11187(a, middle, tail) = new(a, middle..., tail...)
end
mixed11187 = mixed_splat_new11187(1, (2, 3), (4,))
@test mixed11187.a == 1
@test mixed11187.b == 2
@test mixed11187.c == 3
@test mixed11187.d == 4

# The upstream Rational shape: a normalizing inner constructor plus a global
# unchecked terminal constructor over the same struct.
struct MyRat11005{T<:Integer} <: Real
    num::T
    den::T
    function MyRat11005{T}(num::T, den::T) where {T<:Integer}
        den == 0 && error("zero denominator")
        g = gcd(num, den)
        new{T}(div(num, g), div(den, g))
    end
    global unsafe_myrat11005(::Type{T}, num, den) where {T} = new{T}(num, den)
end

normalized11005 = MyRat11005{Int}(4, 6)
@test normalized11005.num == 2
@test normalized11005.den == 3

unchecked11005 = unsafe_myrat11005(Int, 4, 6)
@test typeof(unchecked11005) === MyRat11005{Int}
@test unchecked11005.num == 4
@test unchecked11005.den == 6

true
