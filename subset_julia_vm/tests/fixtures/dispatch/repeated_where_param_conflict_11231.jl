using Test

struct Trip3{A,B,C} end
struct Boxx{T} end
struct Wrap2{A,B} end

same_pair(::Type{Pair{T,T}}) where T = true
same_pair(::Type) = false

same_nested(::Type{Wrap2{T,Boxx{T}}}) where T = true
same_nested(::Type) = false

same_trip3(::Type{Trip3{T,T,T}}) where T = true
same_trip3(::Type) = false

same_tup(::Type{Tuple{T,T}}) where T = true
same_tup(::Type) = false

@testset "repeated where parameter rejects conflicting binding (Issue #11231)" begin
    # Pair{T,T}: repeated type parameter in two invariant slots. The legacy
    # struct-to-struct binding extractor inserted each positional binding into
    # a HashMap without checking for a prior conflicting binding, so
    # Pair{Int,String} silently overwrote T=Int with T=String and matched.
    @test same_pair(Pair{Int,String}) == false
    @test same_pair(Pair{Int,Int}) == true
    runtime_same_pair = same_pair
    @test runtime_same_pair(Pair{Int,String}) == false
    @test runtime_same_pair(Pair{Int,Int}) == true

    # Nested: Wrap2{T,Boxx{T}} repeats T across a plain slot and a nested
    # parametric struct slot (the recursive nested-binding merge path,
    # Issue #8853's merge point).
    @test same_nested(Wrap2{Int,Boxx{Int}}) == true
    @test same_nested(Wrap2{Int,Boxx{String}}) == false
    runtime_same_nested = same_nested
    @test runtime_same_nested(Wrap2{Int,Boxx{Int}}) == true
    @test runtime_same_nested(Wrap2{Int,Boxx{String}}) == false

    # Three occurrences of the same where parameter: every pairwise conflict
    # must reject the candidate, not just the first-vs-second slot.
    @test same_trip3(Trip3{Int,Int,Int}) == true
    @test same_trip3(Trip3{Int,Int,String}) == false
    @test same_trip3(Trip3{Int,String,Int}) == false
    @test same_trip3(Trip3{String,Int,Int}) == false
    runtime_same_trip3 = same_trip3
    @test runtime_same_trip3(Trip3{Int,Int,Int}) == true
    @test runtime_same_trip3(Trip3{Int,Int,String}) == false
    @test runtime_same_trip3(Trip3{Int,String,Int}) == false
    @test runtime_same_trip3(Trip3{String,Int,Int}) == false

    # Built-in Tuple{T,T} literal type argument (Issue #11490): a DIFFERENT
    # root cause from the struct-to-struct binding-extraction overwrite above.
    # A literal `Tuple{...}` call argument constant-folds to a single-arg
    # compile-time fast path (`core_static_datatype_exact_match`) that checked
    # each `where`-bound Tuple element independently, so a repeated `T` bound
    # to Int by the first slot silently accepted a different concrete type
    # (String) in the second slot instead of falling back to the generic
    # `::Type` candidate. Only the STATIC call site was affected; runtime
    # dispatch through a variable-bound function value already rejected
    # correctly (the general CoreType-native typemap matcher tracks repeated
    # `where` bindings correctly, only this fast-path shortcut did not).
    @test same_tup(Tuple{Int,Int}) == true
    @test same_tup(Tuple{Int,String}) == false
    runtime_same_tup = same_tup
    @test runtime_same_tup(Tuple{Int,Int}) == true
    @test runtime_same_tup(Tuple{Int,String}) == false
end

true
