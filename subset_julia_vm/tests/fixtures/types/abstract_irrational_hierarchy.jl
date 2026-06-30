# AbstractIrrational <: Real type hierarchy parity (Issue #5134)
#
# Verifies the `AbstractIrrational <: Real` abstract type hierarchy that backs
# Irrational dispatch/promote. Every assertion below matches upstream Julia
# (`julia/base/irrationals.jl`) exactly, so this fixture passes under both
# `sjulia` and official `julia`.
#
# NOTE: This fixture deliberately does NOT assert `typeof(pi)`/`pi isa
# Irrational`. In this VM `pi` is folded to a `Float64` literal by design
# (Issue #533); promoting the built-in `pi` constant to an actual
# `Irrational{:π}` value is tracked separately. Here we only exercise the
# abstract hierarchy itself, which is what Issue #5134 requests.

using Test

# User-defined subtype declared at top level (struct defs cannot be nested
# inside a @testset begin ... end block in this subset).
struct MyIrr{sym} <: AbstractIrrational end
classify(::AbstractIrrational) = "irrational"
classify(::Real) = "real"

@testset "AbstractIrrational <: Real hierarchy (Issue #5134)" begin
    # --- Abstract hierarchy: AbstractIrrational <: Real <: Number ---
    @test AbstractIrrational <: Real
    @test AbstractIrrational <: Number
    @test AbstractIrrational <: Any
    @test supertype(AbstractIrrational) == Real

    # --- Irrational{sym} <: AbstractIrrational ---
    @test Irrational <: AbstractIrrational
    @test Irrational <: Real
    @test Irrational <: Number
    @test supertype(Irrational) == AbstractIrrational

    # --- Parametric instantiations preserve the hierarchy ---
    @test Irrational{:π} <: AbstractIrrational
    @test Irrational{:π} <: Real
    @test Irrational{:π} <: Number
    @test Irrational{:π} <: Irrational
    @test !(Irrational{:e} <: Irrational{:π})
    @test Irrational{:π} <: Irrational{:π}

    # --- A constructed Irrational value satisfies isa across the hierarchy ---
    x = Irrational{:π}()
    @test typeof(x) == Irrational{:π}
    @test x isa Irrational{:π}
    @test x isa Irrational
    @test x isa AbstractIrrational
    @test x isa Real
    @test x isa Number

    # --- The exported `pi` constant is a Real (Issue #5134 fixture) ---
    @test pi isa Real

    # --- User-defined subtypes participate in dispatch as Real ---
    @test MyIrr{:sqrt2} <: AbstractIrrational
    @test MyIrr{:sqrt2} <: Real
    @test MyIrr{:sqrt2}() isa AbstractIrrational
    @test MyIrr{:sqrt2}() isa Real

    # Bug #5582 / broader method specificity work (#5072): the narrower
    # AbstractIrrational method must beat the broader Real fallback.
    @test classify(Irrational{:π}()) == "irrational"
    @test classify(MyIrr{:sqrt2}()) == "irrational"
    @test classify(1.5) == "real"

    # Tuple parametric covariance through the hierarchy
    @test Tuple{Irrational{:π}} <: Tuple{AbstractIrrational}
    @test Tuple{Irrational{:π}} <: Tuple{Real}
end

# Return true to indicate success
true
