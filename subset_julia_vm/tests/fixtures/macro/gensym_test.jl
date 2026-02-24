# Test gensym for generating unique symbols
# gensym() creates unique symbols to avoid variable name collisions

s1 = gensym()
s2 = gensym()
s3 = gensym("foo")

# Verify they are Symbol type
@assert typeof(s1) == Symbol
@assert typeof(s2) == Symbol
@assert typeof(s3) == Symbol

# Return success
Float64(42)
