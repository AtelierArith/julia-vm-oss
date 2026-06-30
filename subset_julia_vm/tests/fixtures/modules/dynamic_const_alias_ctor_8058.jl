using Test

module A8058
    struct Bar8058; x; end                       # default field constructor only
    struct Baz8058; y; Baz8058(y) = new(y); end  # inner constructor
    export Bar8058, Baz8058
end
module B8058
    import ..A8058
    const Bar8058 = A8058.Bar8058
    const Baz8058 = A8058.Baz8058
    export Bar8058, Baz8058
end
using .B8058

@testset "dynamic call of using-imported const type alias (Issue #8058)" begin
    @test Bar8058(7).x == 7          # static (already worked via #8049)
    t = Bar8058
    @test t(7).x == 7                # dynamic local-var call (the fix)
    @test t === A8058.Bar8058

    s = Baz8058                       # inner-ctor struct still works dynamically
    @test s(9).y == 9

    u = A8058.Bar8058                 # qualified alias value dynamic call
    @test u(11).x == 11
end

true
