# Test ispublic and isexported functions

using Test

# Test with a user-defined module that has exports and public declarations
module TestMod
    export foo
    public bar

    function foo()
        "foo"
    end

    function bar()
        "bar"
    end

    function baz()
        "baz"
    end
end

@testset "isexported and ispublic" begin
    # Test isexported
    @test Base.isexported(TestMod, :foo) == true
    @test Base.isexported(TestMod, :bar) == false
    @test Base.isexported(TestMod, :baz) == false

    # Test ispublic - exported symbols are also public
    @test Base.ispublic(TestMod, :foo) == true
    @test Base.ispublic(TestMod, :bar) == true
    @test Base.ispublic(TestMod, :baz) == false
end

true  # Test passed
