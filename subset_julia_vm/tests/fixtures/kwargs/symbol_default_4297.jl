using Test

function kw_symbol_default_probe_4297(; generated=true, debuginfo=:default)
    return (generated, typeof(generated), debuginfo, typeof(debuginfo))
end

function kw_symbol_default_explicit_4297(; generated=true, debuginfo=:default)
    return (generated, typeof(generated), debuginfo, typeof(debuginfo))
end

@testset "symbol keyword defaults preserve Symbol values (Issue #4297)" begin
    omitted = kw_symbol_default_probe_4297()
    explicit = kw_symbol_default_explicit_4297(debuginfo=:none)

    @test omitted[1] == true
    @test omitted[2] === Bool
    @test omitted[3] === :default
    @test omitted[4] === Symbol

    @test explicit[1] == true
    @test explicit[2] === Bool
    @test explicit[3] === :none
    @test explicit[4] === Symbol
end

true
