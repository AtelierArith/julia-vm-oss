# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: macros/colon_hex_type_4927.jl =====
# Issue #4927: `:0xFF` evaluated to `Int64(255)` instead of
# `UInt8(0xFF)`. Bare `0xFF` correctly produced `UInt8`, so the type
# information was lost specifically on the colon-prefix-of-literal
# path added in PR #4926 (Issue #4923).
#
# Root cause: the `cst_to_expr_constructor::IntegerLiteral` arm used
# `parse_int` (untyped), which discarded the hex / binary / octal
# width tag. The bare-literal lowering elsewhere uses
# `lower_integer_literal` (`parse_int_typed` + wrap in
# `UInt8 / UInt16 / …` constructor when the kind is set).
#
# Fix: in `lowering/expr/quote/cst_to_constructor.rs`, route the
# `NodeKind::IntegerLiteral` arm through the same
# `lower_integer_literal` helper. `lower_integer_literal` is now
# `pub(super)` so the quote module can reuse it.


@testset "colon-prefix hex literal preserves UInt type (Issue #4927)" begin
    @test :0xFF === 0xFF
    @test typeof(:0xFF) === UInt8

    @test :0x100 === 0x100
    @test typeof(:0x100) === UInt16

    @test :0x10000 === 0x10000
    @test typeof(:0x10000) === UInt32
end

@testset "colon-prefix binary literal preserves UInt type (Issue #4927)" begin
    @test :0b1010 === 0b1010
    @test typeof(:0b1010) === UInt8

    @test :0b1_0000_0000 === 0b1_0000_0000
    @test typeof(:0b1_0000_0000) === UInt16
end

@testset "decimal :literal stays Int64 (regression guard, Issue #4927)" begin
    @test :42 === 42
    @test typeof(:42) === Int64
    @test :0 === 0
end

# ===== source: macros/colon_literal_4923.jl =====
# Issue #4923: `:42`, `:3.14`, `:"hello"`, `:'A'`, `:true` — the
# colon-prefix syntax applied to a literal — was rejected by the
# parser. Upstream Julia treats `:literal` as `QuoteNode(literal)`,
# which at the *top level* evaluates immediately to the literal value
# (`typeof(:42) === Int64`, not QuoteNode). Inside a nested quote
# `:(:42)`, the result is `QuoteNode(42)` as the embedded AST.
#
# Fix: two parts —
#   1. In `subset_julia_vm_parser/src/parser/expressions/primary.rs`,
#      add an arm in `parse_colon_prefix` for numeric / char / string
#      literal tokens that produces a `QuoteExpression` CST node with
#      the literal as a child (with-children form).
#   2. In `subset_julia_vm_lowering/src/lowering/expr/quote/cst_to_constructor.rs`,
#      `lower_quote_expr` now branches on `children.is_empty()` rather
#      than the text-shape heuristic. The with-children path returns
#      the inner literal's constructor directly (so top-level `:42`
#      evaluates to `42`). Also adds a `NodeKind::CharacterLiteral`
#      arm in `cst_to_expr_constructor` (was missing).
#
# The nested case `:(:42)` continues to produce `QuoteNode(42)` via
# the `NodeKind::QuoteExpression` recursion arm added in PR #4914 /
# #4920.


@testset "colon-prefix on integer literal (Issue #4923)" begin
    @test :42 === 42
    @test typeof(:42) === Int64
    # `:0xFF` and `:0b1010` parse and evaluate correctly to 255 / 10
    # but as `Int64` instead of `UInt8`. That's an orthogonal hex /
    # binary literal-type-preservation gap, out of scope here.
    @test :0xFF == 0xFF
    @test :0b1010 == 0b1010
end

@testset "colon-prefix on float literal (Issue #4923)" begin
    @test :3.14 === 3.14
    @test typeof(:3.14) === Float64
end

@testset "colon-prefix on string literal (Issue #4923)" begin
    @test :"hello" == "hello"
    @test typeof(:"hello") === String
end

@testset "colon-prefix on char literal (Issue #4923)" begin
    @test :'A' === 'A'
    @test typeof(:'A') === Char
end

@testset "colon-prefix binds tightly (regression guard, Issue #4923)" begin
    # `:1 + 2` is `:1 + 2` = `1 + 2` = 3, not `:(1 + 2)`.
    @test :1 + 2 == 3
    @test :2 * 3 == 6
end

@testset "nested :(:literal) still produces QuoteNode (Issue #4911/#4920 guard)" begin
    @test :(:42) isa QuoteNode
    @test :(:foo) isa QuoteNode
end

# ===== source: macros/inbounds.jl =====
# Test @inbounds macro (related to Issue #890)
# - @inbounds marks local indexing expressions for inbounds codegen (Issue #4286)
# - @inbounds expr should execute normally
# - @inbounds for loop should execute normally


@testset "@inbounds with for loop" begin
    arr = [1, 2, 3, 4, 5]
    sum = 0
    @inbounds for i in 1:length(arr)
        sum += arr[i]
    end
    @test sum == 15
end

@testset "@inbounds with array mutation" begin
    arr = zeros(Int, 5)
    @inbounds for i in 1:5
        arr[i] = i * 10
    end
    @test arr == [10, 20, 30, 40, 50]
end

@testset "@inbounds statement body indexing" begin
    vals = Float64[1.0, 2.0, 3.0]
    idxs = [3, 1]
    @inbounds for i in idxs
        vals[i] = vals[i] + 10.0
    end
    @test vals == Float64[11.0, 2.0, 13.0]
end

@testset "@inbounds with direct indexing expressions" begin
    arr = Int32[10, 20, 30]
    @test @inbounds arr[2] == Int32(20)
    @test @inbounds getindex(arr, 3) == Int32(30)
    @test @inbounds Base.getindex(arr, 1) == Int32(10)

    vals = Float64[1.0, 2.0, 3.0]
    @inbounds vals[2] = 4.5
    @test vals == Float64[1.0, 4.5, 3.0]
    @inbounds setindex!(vals, 6.5, 3)
    @test vals == Float64[1.0, 4.5, 6.5]
end

@testset "@inbounds with while loop" begin
    sum = 0
    i = 1
    @inbounds while i <= 5
        sum += i
        i += 1
    end
    @test sum == 15
end

@testset "@inbounds with if statement" begin
    x = 10
    result = 0
    @inbounds if x > 5
        result = x * 2
    end
    @test result == 20
end

# ===== source: macros/line_module_macros.jl =====
# Test @__LINE__ and @__MODULE__ macros
# These macros return information about the source location at compile time


@testset "@__LINE__ and @__MODULE__ macros" begin
    # @__LINE__ returns the current line number as an integer
    # The exact line depends on where the macro is called
    line1 = @__LINE__
    @test typeof(line1) == Int64
    @test line1 > 0

    # Consecutive lines should have increasing line numbers
    line2 = @__LINE__
    @test line2 > line1

    # @__MODULE__ returns the current module
    mod = @__MODULE__
    # Check that it's a Module type by checking its string representation
    mod_str = string(mod)
    @test mod_str == "Main"
end

# ===== source: macros/macro_arg_space_paren_5494.jl =====

# Issue #5494: in space-separated macro arguments, a space before `(` separates
# arguments instead of fusing into a call. So `@m Ident (expr)` is two arguments
# (`Ident` and `(expr)`), matching upstream Julia, NOT one call `Ident(expr)`.
#
# The canonical failure was `@test_throws TypeError (1 + 1)::Float64`, which was
# (mis)parsed as the single argument `(TypeError(1 + 1))::Float64`. The macro
# then saw only one argument and failed. With the fix it parses as the expected
# two arguments (`TypeError` and `(1 + 1)::Float64`) and the typed assert below
# correctly throws a TypeError.

@testset "macros_macro_arg_space_paren_5494 typed throws" begin
    # `(1 + 1)::Float64` is `2::Float64`, which throws a TypeError because the
    # Int value 2 is not a Float64. `@test_throws` must receive TypeError as its
    # first argument and the typed expression as its second.
    @test_throws TypeError (1 + 1)::Float64

    # A plain parenthesized expression as the second argument, with a space
    # before `(`, must still be a separate argument (not a call `TypeError(...)`).
    @test_throws TypeError (10 + 5)::Float64
end

@testset "macros_macro_arg_space_paren_5494 no-space call still fuses" begin
    # Without a space, `error("boom")` is a single call argument and is invoked,
    # so `@test_throws` sees the thrown ErrorException. This pins that the fix
    # does NOT break adjacent macro-argument calls.
    @test_throws ErrorException error("boom")
end

# ===== source: macros/simd.jl =====
# Test @simd macro (Issue #890)
# - @simd is a no-op in SubsetJuliaVM (no JIT/LLVM vectorization)
# - @simd for loop should execute normally
# - @simd ivdep for loop should also execute normally


@testset "@simd basic for loop" begin
    # Simple sum with @simd
    sum = 0
    @simd for i in 1:10
        sum += i
    end
    @test sum == 55
end

@testset "@simd with array accumulation" begin
    # Sum array elements with @simd
    arr = [1, 2, 3, 4, 5]
    total = 0
    @simd for i in 1:length(arr)
        total += arr[i]
    end
    @test total == 15
end

@testset "@simd with computation" begin
    # Compute squares with @simd
    squares = zeros(Int, 5)
    @simd for i in 1:5
        squares[i] = i * i
    end
    @test squares == [1, 4, 9, 16, 25]
end

@testset "@simd ivdep variant" begin
    # @simd ivdep should also work (no-op in SubsetJuliaVM)
    sum = 0
    @simd ivdep for i in 1:5
        sum += i
    end
    @test sum == 15
end

@testset "@simd with nested computation" begin
    # More complex computation
    result = 0.0
    @simd for i in 1:4
        result += i * 2.5
    end
    # 1*2.5 + 2*2.5 + 3*2.5 + 4*2.5 = 2.5 + 5.0 + 7.5 + 10.0 = 25.0
    @test result == 25.0
end

# ===== source: macros/test_showtime.jl =====
# Test @showtime macro - timing with expression display


@testset "@showtime macro" begin
    # Test that @showtime returns the correct value
    result = @showtime begin
        x = 0
        for i in 1:100
            x = x + i
        end
        x
    end
    @test result == 5050  # Sum of 1..100

    # Test simple expression
    val = @showtime 1 + 2
    @test val == 3
end

# ===== source: macros/test_something_coalesce.jl =====
# Test @something and @coalesce macros


@testset "@something and @coalesce macros" begin
    # @something tests - returns first non-nothing value
    @test @something(nothing, 42) == 42
    @test @something(1, 2, 3) == 1
    @test @something(nothing, nothing, 99) == 99

    # @coalesce tests - returns first non-missing value
    @test @coalesce(missing, 42) == 42
    @test @coalesce(1, 2, 3) == 1
    @test @coalesce(missing, missing, 99) == 99

    # Test with expressions
    x = nothing
    @test @something(x, 100) == 100

    y = missing
    @test @coalesce(y, 200) == 200
end

# ===== source: macros/test_timev.jl =====
# Test @timev macro - verbose timing output


@testset "@timev macro" begin
    # Test basic @timev usage (single argument form)
    result = @timev begin
        x = 0
        for i in 1:100
            x = x + i
        end
        x
    end
    @test result == 5050  # Sum of 1..100
end

# ===== source: macros/testset_function.jl =====
# Test function definitions inside @testset (Issue #902)
# Functions defined inside macro blocks should be callable within the same scope


# Test basic function definition inside @testset
@testset "function inside testset" begin
    function f(x)
        2x
    end
    @test f(2) == 4
    @test f(5) == 10
end

# Test short function definition inside @testset
@testset "short function inside testset" begin
    g(x) = x * x
    @test g(3) == 9
    @test g(4) == 16
end

# Test multiple functions inside @testset
@testset "multiple functions inside testset" begin
    function add(a, b)
        a + b
    end
    function mul(a, b)
        a * b
    end
    @test add(2, 3) == 5
    @test mul(2, 3) == 6
end

# Test function with typed parameters inside @testset
@testset "typed function inside testset" begin
    function typed_add(x::Int, y::Int)
        x + y
    end
    @test typed_add(10, 20) == 30
end

# ===== source: macros/timev_macro.jl =====
# Test @timev macro - verbose timing measurement
# @timev prints elapsed time in seconds and nanoseconds


@testset "@timev macro" begin
    # @timev returns the value of the expression
    result = @timev sum(1:10)
    @test result == 55

    # @timev with a simple expression
    x = @timev 1 + 2 + 3
    @test x == 6

    # Test that the value is correctly returned
    y = @timev 2 * 21
    @test y == 42
end

true
