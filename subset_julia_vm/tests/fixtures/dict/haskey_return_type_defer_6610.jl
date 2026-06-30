# Issue #6610: Bool/collection-returning Base ops (`haskey`, `isempty`, `empty!`)
# overridden on a custom type with a different return type, then called through
# an `Any`-typed binding, were coerced to the inferred return type (crashing
# with `ReturnI64` or constant-folding a String comparison to `false`) instead
# of dispatching to the user method's value. Each op's return type was pinned in
# the tfunc registry (read by the abstract-interp engine) regardless of the
# receiver; they now defer for a struct/unknown receiver while concrete built-in
# collections keep their precise return type. Verified against upstream Julia 1.12.

using Test

struct Tagged6610
    tag::String
end

Base.haskey(x::Tagged6610, k) = "haskey:" * x.tag
Base.isempty(x::Tagged6610) = "isempty:" * x.tag
Base.empty!(x::Tagged6610) = "empty!:" * x.tag

# Receivers are Any-typed, so the overrides must be reached at runtime, not
# coerced. The String comparison is the observable symptom: if the call is
# wrongly inferred to return Bool/the-collection, `<non-String> == String`
# constant-folds to `false` (the #6539-class equality shortcut).
call_haskey(x) = haskey(x, "k")
call_isempty(x) = isempty(x)
call_empty!(x) = empty!(x)

# Built-in collections keep their precise return type (no fast-path regression).
concrete_haskey(d) = haskey(d, "a")
concrete_isempty(d) = isempty(d)

ok_haskey() = call_haskey(Tagged6610("T")) == "haskey:T"
ok_isempty() = call_isempty(Tagged6610("T")) == "isempty:T"
ok_empty() = call_empty!(Tagged6610("T")) == "empty!:T"
ok_concrete() =
    concrete_haskey(Dict("a" => 1)) === true &&
    concrete_haskey(Dict("b" => 1)) === false &&
    concrete_isempty(Dict{String,Int64}()) === true &&
    concrete_isempty(Dict("a" => 1)) === false

@testset "haskey/isempty/empty! return-type defer through Any binding (#6610)" begin
    @test ok_haskey()
    @test ok_isempty()
    @test ok_empty()
    @test ok_concrete()
end

# Final value gates the in-harness nextest run on correctness, not just no-throw.
ok_haskey() && ok_isempty() && ok_empty() && ok_concrete()
