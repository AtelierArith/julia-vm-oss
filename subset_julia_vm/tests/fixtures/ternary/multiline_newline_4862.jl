# Issue #4862: The parser rejected a newline immediately after `?` in a
# multi-line ternary expression with
# `ParseFailed("unexpected token '\n' ..., expected expression")`.
# Upstream Julia treats `?` as a line-continuation token — same as
# `&&`, `||`, `=>`, and the arithmetic / comparison operators.
#
# The binary-operator continuation loop in
# `subset_julia_vm_parser/src/parser/expressions/mod.rs` already lists
# `?` in its accept-continuation comment, but the ternary dispatch
# path bypasses that loop and goes directly to `parse_ternary`, which
# was not eating the newlines on its own. The fix adds a
# `while self.check(&Token::Newline) { self.advance(); }` block after
# consuming `?` (and after the inner `:`, for the symmetric
# `cond ? then :\n else` form).

using Test

# Newline immediately after `?`
f1_4862(x) =
    x > 0 ?
        x : 0

# Newline immediately after `:`
f2_4862(x) =
    x > 0 ? x :
        0

# Newlines after BOTH `?` and `:`
f3_4862(x) =
    x > 0 ?
        x :
        0

# Nested multi-line ternary
f4_4862(x, y) =
    x > 0 ?
        (y > 0 ?
            x + y :
            x) :
        0

# Multi-line ternary inside an explicit function body
function f6_4862(x)
    return x > 0 ?
        "positive" :
        "non-positive"
end

@testset "Newline after `?` parses (Issue #4862)" begin
    @test f1_4862(5) == 5
    @test f1_4862(-1) == 0
end

@testset "Newline after `:` parses (Issue #4862)" begin
    @test f2_4862(5) == 5
    @test f2_4862(-1) == 0
end

@testset "Newlines after both `?` and `:` parse (Issue #4862)" begin
    @test f3_4862(5) == 5
    @test f3_4862(-1) == 0
end

@testset "Nested multi-line ternary parses (Issue #4862)" begin
    @test f4_4862(5, 3) == 8
    @test f4_4862(5, -1) == 5
    @test f4_4862(-1, 3) == 0
end

@testset "Multi-line ternary inside function body parses (Issue #4862)" begin
    @test f6_4862(5) == "positive"
    @test f6_4862(-1) == "non-positive"
end

@testset "Single-line ternary still works (regression guard)" begin
    f5_4862(x) = x > 0 ? x : 0
    @test f5_4862(5) == 5
    @test f5_4862(-1) == 0
end

true
