using Test

# Regression test for Issue #8869:
# A Symbol-valued type parameter (e.g. Foo{:hello}) was bound as a DataType
# wrapper instead of the raw Symbol value. bind_type_params now materializes
# symbol type-parameter values via strip_prefix(':').

struct TaggedFoo{sym} end

function describe(::TaggedFoo{sym}) where {sym}
    return sym
end

x = TaggedFoo{:hello}()
result = describe(x)

# sym should be the Symbol :hello, not a DataType
@test result == :hello
@test result isa Symbol
@test result !== :world

# Multiple different symbol tags
y = TaggedFoo{:world}()
@test describe(y) == :world

# Integer value type params still work
struct Bar{N} end
function getN(::Bar{N}) where {N}
    return N
end
@test getN(Bar{42}()) == 42

true
