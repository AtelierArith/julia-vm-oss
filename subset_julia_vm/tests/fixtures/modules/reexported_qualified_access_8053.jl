using Test

module Defn8053
    struct T8053; v; T8053() = new(7); end
    g8053(a::T8053) = a.v
    export T8053, g8053
end

# Selective re-export via `import ..Mod: names`.
module Facade8053
    import ..Defn8053: T8053, g8053
    export T8053, g8053
end

# Chained re-export (Facade2 re-exports Facade8053's re-exported bindings).
module Facade2_8053
    using ..Facade8053: T8053, g8053
    export T8053, g8053
end

using .Facade8053

@testset "qualified access to re-exported binding (Issue #8053)" begin
    t = T8053()
    @test t isa T8053                       # unqualified (already worked)
    @test isdefined(Facade8053, :T8053)
    @test t isa Facade8053.T8053            # qualified type access (fix)
    @test Facade8053.T8053 === Defn8053.T8053
    @test Facade8053.g8053(t) == 7          # qualified call (fix)
    # chained re-export resolves to the original source
    @test Facade2_8053.T8053 === Defn8053.T8053
    @test Facade2_8053.g8053(t) == 7
end

true
