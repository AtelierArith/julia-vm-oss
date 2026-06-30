using Test

macro quote_const_declaration_5578(sym)
    quote
        const $(esc(sym)) = 5578
    end
end

@quote_const_declaration_5578 quote_const_value_5578

@testset "quote const declaration in macro expansion (Issue #5578)" begin
    @test quote_const_value_5578 == 5578
end

true
