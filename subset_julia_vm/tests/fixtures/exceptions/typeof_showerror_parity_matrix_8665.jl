# typeof(e)/showerror parity fixture matrix (Issue #8665, parent #8643)
#
# For each VmError variant that can be triggered from Julia user code:
# 1. Verify typeof(e) matches upstream Julia (the primary correctness goal)
# 2. Verify sprint(showerror, e) — exact match or close approximation
#
# Variants that still have showerror divergence are marked A' and have
# a tracking Issue. Variants that are not yet catchable are omitted here
# and tracked in their own Issues.
#
# Known gaps NOT tested here (separate Issues filed):
#   - EmptyArrayPop (pop!(Int[])) not catchable: Issue #8744
#   - UndefKeywordError (f(; kw)=kw; f()) not catchable: Issue #8745
#   - MethodError showerror double-prefix: Issue #8748 (A' accepted for now)
#   - DomainError val=nothing placeholder: A' (tracking only)
#   - BoundsError for range/tuple showerror minimal: A' (tracking only)
using Test

# Struct defs must be at top level (sjulia does not support struct inside @testset)
struct ImmFix8665; x::Int; end
function destructure_mismatch_8665()
    (a,b,c) = (1,2)
    a
end
function overflow_8665(); overflow_8665(); end

@testset "typeof(e)/showerror parity matrix (Issue #8665)" begin

    # ── DivideError ────────────────────────────────────────────────────────
    e = try; div(1, 0); catch ex; ex; end
    @test typeof(e) == DivideError
    @test e isa DivideError
    @test e isa Exception
    @test sprint(showerror, e) == "DivideError: integer division error"

    # ── BoundsError (array) ────────────────────────────────────────────────
    e = try; [1,2,3][10]; catch ex; ex; end
    @test typeof(e) == BoundsError
    @test e isa BoundsError
    @test e isa Exception

    # ── BoundsError (range index) ──────────────────────────────────────────
    e = try; (1:3)[10]; catch ex; ex; end
    @test typeof(e) == BoundsError
    @test e isa BoundsError

    # ── BoundsError (tuple index) ─────────────────────────────────────────
    e = try; (1,2)[5]; catch ex; ex; end
    @test typeof(e) == BoundsError
    @test e isa BoundsError

    # ── BoundsError (field index out of bounds) ───────────────────────────
    e = try; getfield((x=1,y=2), 10); catch ex; ex; end
    @test typeof(e) == BoundsError
    @test e isa BoundsError

    # ── BoundsError (tuple destructuring) ─────────────────────────────────
    e = try; destructure_mismatch_8665(); catch ex; ex; end
    @test typeof(e) == BoundsError
    @test e isa BoundsError

    # ── InexactError ───────────────────────────────────────────────────────
    e = try; convert(Int64, 1.5); catch ex; ex; end
    @test typeof(e) == InexactError
    @test e isa InexactError
    @test e isa Exception
    @test sprint(showerror, e) == "InexactError: Int64(1.5)"

    # ── DomainError ────────────────────────────────────────────────────────
    # A': typeof correct; showerror shows "with nothing:" instead of "with -1.0:"
    e = try; sqrt(-1); catch ex; ex; end
    @test typeof(e) == DomainError
    @test e isa DomainError
    @test e isa Exception

    # ── MethodError ────────────────────────────────────────────────────────
    # A': typeof correct; showerror has double-prefix (Issue #8748)
    e = try; abs("str"); catch ex; ex; end
    @test typeof(e) == MethodError
    @test e isa MethodError
    @test e isa Exception

    # ── ErrorException (explicit throw) ───────────────────────────────────
    e = try; throw(ErrorException("test msg")); catch ex; ex; end
    @test typeof(e) == ErrorException
    @test e isa ErrorException
    @test sprint(showerror, e) == "test msg"

    # ── ErrorException (error() function) ─────────────────────────────────
    e = try; error("boom"); catch ex; ex; end
    @test typeof(e) == ErrorException
    @test e isa ErrorException
    @test sprint(showerror, e) == "boom"

    # ── DimensionMismatch (matrix multiplication) ─────────────────────────
    e = try; [1 2; 3 4] * [1; 2; 3]; catch ex; ex; end
    @test typeof(e) == DimensionMismatch
    @test e isa DimensionMismatch
    @test e isa Exception

    # ── ArgumentError (empty tuple first()) ───────────────────────────────
    e = try; first(()); catch ex; ex; end
    @test typeof(e) == ArgumentError
    @test e isa ArgumentError
    @test sprint(showerror, e) == "ArgumentError: tuple must be non-empty"

    # ── ErrorException (setfield! on immutable) ───────────────────────────
    e = try; setfield!(ImmFix8665(1), :x, 2); catch ex; ex; end
    @test typeof(e) == ErrorException
    @test e isa ErrorException
    @test startswith(sprint(showerror, e), "setfield!: immutable struct")

    # ── StringIndexError ───────────────────────────────────────────────────
    e = try; "αβγ"[2]; catch ex; ex; end
    @test typeof(e) == StringIndexError
    @test e isa StringIndexError
    @test e isa Exception
    # Both julia and sjulia include "invalid index [2]"; upstream also shows nearby
    # valid indices but the exact format differs by sjulia version (A' quality).
    @test startswith(sprint(showerror, e), "StringIndexError: invalid index [2]")

    # ── FieldError (NamedTuple missing field) ─────────────────────────────
    e = try; (x=1, y=2).z; catch ex; ex; end
    @test typeof(e) == FieldError
    @test e isa FieldError
    @test e isa Exception
    se = sprint(showerror, e)
    @test startswith(se, "FieldError:")
    @test occursin("z", se)

    # ── UndefVarError ──────────────────────────────────────────────────────
    e = try; some_undef_var_8665_xyz; catch ex; ex; end
    @test typeof(e) == UndefVarError
    @test e isa UndefVarError
    @test e isa Exception
    se = sprint(showerror, e)
    @test occursin("some_undef_var_8665_xyz", se)

    # ── StackOverflowError ─────────────────────────────────────────────────
    e = try; overflow_8665(); catch ex; ex; end
    @test typeof(e) == StackOverflowError
    @test e isa StackOverflowError
    @test e isa Exception

    # ── KeyError ───────────────────────────────────────────────────────────
    # Note: the Dict getindex path throws ErrorException("KeyError: key not found")
    # instead of KeyError — tracked in Issue #8747. Test the throw path instead.
    e = try; throw(KeyError("missing_key_8665")); catch ex; ex; end
    @test typeof(e) == KeyError
    @test e isa KeyError
    # Upstream quotes String keys; sjulia showerror uses the raw string.
    # Check that the key name appears in the message regardless of quoting.
    @test occursin("missing_key_8665", sprint(showerror, e))

    # ── UndefRefError (Core.Binding unset field, Issue #10067) ─────────────
    e = try
        gr_10067 = GlobalRef(Main, :sin)
        b_10067 = getfield(gr_10067, :binding)
        getfield(b_10067, :value)
    catch ex
        ex
    end
    @test typeof(e) == UndefRefError
    @test e isa UndefRefError
    @test e isa Exception
    @test sprint(showerror, e) == "UndefRefError: access to undefined reference"

    # ── OverflowError (factorial past the Int64 table) ─────────────────────
    # factorial(21) overflows Int64 and now throws OverflowError with the exact
    # upstream message rather than silently wrapping (Issue #9326).
    e = try; factorial(21); catch ex; ex; end
    @test typeof(e) == OverflowError
    @test e isa OverflowError
    @test e isa Exception
    @test sprint(showerror, e) ==
          "OverflowError: 21 is too large to look up in the table; consider using `factorial(big(21))` instead"

end
true
