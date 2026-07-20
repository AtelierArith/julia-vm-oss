# Test macro dispatch with Int literal type
# @m 42 matches ::Int (literal), @m x does NOT (Symbol)

macro process(x::Int)
    :(1)  # Int literal
end

macro process(x::Symbol)
    :(2)  # Symbol (variable name)
end

macro process(x::Expr)
    :(3)  # Expression
end

r1 = @process(42)      # Int literal → 1
r2 = @process(x)       # Symbol → 2
r3 = @process(1 + 2)   # Expr → 3

# Julia semantics: even if x = 42, @process(x) returns 2 (Symbol), not 1
r1 + r2 + r3  # Expected: 6
