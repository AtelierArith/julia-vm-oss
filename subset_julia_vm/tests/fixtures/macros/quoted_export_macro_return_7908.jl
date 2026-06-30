using Test

module QuotedExportMacroReturn7908
macro alias(alias_name::Symbol, real_name::Symbol)
    result = quote
        if isdefined($__module__, $(QuoteNode(alias_name)))
            $alias_name === $real_name || error("Alias mismatch")
        else
            const $alias_name = $real_name
        end
        export $alias_name
    end
    return esc(result)
end

function foo end
@alias bar foo
end

using .QuotedExportMacroReturn7908

@testset "quoted export statements lower through macro return (Issue #7908)" begin
    @test isdefined(Main, :bar)
    @test !isdefined(Main, :baz)
end

true
