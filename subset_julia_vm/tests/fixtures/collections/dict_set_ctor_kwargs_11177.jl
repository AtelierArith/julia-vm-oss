using Test

@testset "explicit parametric Dict/Set constructors reject keyword arguments (Issue #11177)" begin
    call_count = Ref(0)
    function record_kw()
        call_count[] += 1
        return 2
    end

    # `Base.Dict{K,V}(...)` explicit qualified form.
    call_count[] = 0
    @test_throws MethodError Base.Dict{Symbol,Int}(:a => 1; bad = record_kw())
    @test call_count[] == 1

    # Bare `Dict{K,V}(...)` explicit form (unqualified).
    call_count[] = 0
    @test_throws MethodError Dict{Symbol,Int}(:a => 1; bad = record_kw())
    @test call_count[] == 1

    # `Base.Set{T}(...)` explicit qualified form.
    call_count[] = 0
    @test_throws MethodError Base.Set{Int}([1, 2, 3]; bad = record_kw())
    @test call_count[] == 1

    # Bare `Set{T}(...)` explicit form.
    call_count[] = 0
    @test_throws MethodError Set{Int}([1, 2, 3]; bad = record_kw())
    @test call_count[] == 1

    # Keyword-splat form (`; kw...`) must be rejected the same way.
    call_count[] = 0
    kwsplat = (bad = record_kw(),)
    @test_throws MethodError Base.Dict{Symbol,Int}(:a => 1; kwsplat...)
    @test call_count[] == 1

    call_count[] = 0
    kwsplat2 = (bad = record_kw(),)
    @test_throws MethodError Base.Set{Int}([1, 2, 3]; kwsplat2...)
    @test call_count[] == 1

    # Sanity: the no-kwargs explicit forms still construct normally.
    d = Base.Dict{Symbol,Int}(:a => 1)
    @test d[:a] == 1
    @test typeof(d) === Dict{Symbol,Int}

    s = Base.Set{Int}([1, 2, 3])
    @test typeof(s) === Set{Int}
    @test s == Set([1, 2, 3])
end

true
