# Issue #5067: structured (tuple / nested) value type parameters for Val{...}.
#
# Upstream Julia allows any isbits / Symbol / Tuple value as a type parameter,
# so `Val{(1, 2)}`, `Val{(:a, :b)}`, and `Val{(1, (2, 3))}` are all valid
# DataTypes. typeof renders the tuple parameter with a space after each comma
# (`Val{(1, 2)}`), while a source literal may omit it (`Val{(1,2)}`); upstream
# treats both spellings as the same DataType, so `isa` must ignore that
# cosmetic comma spacing.
using Test

# Dispatch on tuple value parameters.
f(::Val{(1, 2)}) = "onetwo"
f(::Val{(3, 4)}) = "threefour"

# Dispatch on symbol-tuple value parameters.
g(::Val{(:a, :b)}) = "ab"
g(::Val{(:c, :d)}) = "cd"

# Dispatch on a nested-tuple value parameter.
h(::Val{(1, (2, 3))}) = "nested"

@testset "Val tuple/nested value parameters (Issue #5067)" begin
    # typeof display renders ", " between tuple elements, matching upstream.
    @assert string(typeof(Val{(1, 2)}())) == "Val{(1, 2)}"
    @assert string(typeof(Val{(:a, :b)}())) == "Val{(:a, :b)}"
    @assert string(typeof(Val{(1, (2, 3))}())) == "Val{(1, (2, 3))}"

    # isa is insensitive to the cosmetic space after each comma.
    @assert Val{(1, 2)}() isa Val{(1,2)}
    @assert Val{(1,2)}() isa Val{(1, 2)}
    @assert Val{(:a, :b)}() isa Val{(:a,:b)}
    @assert Val{(:a,:b)}() isa Val{(:a, :b)}
    @assert Val{(1, (2, 3))}() isa Val{(1,(2,3))}

    # Distinct tuple parameters are distinct DataTypes.
    @assert !(Val{(1, 2)}() isa Val{(1, 3)})
    @assert !(Val{(:a, :b)}() isa Val{(:a, :c)})

    # Dispatch selects the method whose tuple value parameter matches.
    @assert f(Val{(1, 2)}()) == "onetwo"
    @assert f(Val{(3, 4)}()) == "threefour"
    @assert g(Val{(:a, :b)}()) == "ab"
    @assert g(Val{(:c, :d)}()) == "cd"
    @assert h(Val{(1, (2, 3))}()) == "nested"

    @test true
end

true
