using Test

module ConditionalExport7959

if true
    export y
end

if false
    export hidden
end

y = 2
hidden = 3

end

module ConditionalExportSourceOrder7959

if :future_export_7959 in names(@__MODULE__)
    export leaked_future_export_7959
end

export future_export_7959

future_export_7959 = 10
leaked_future_export_7959 = 20

end

using .ConditionalExport7959
using .ConditionalExportSourceOrder7959

@testset "conditional module exports (Issue #7959)" begin
    module_names = names(ConditionalExport7959)
    @test :y in module_names
    @test !(:hidden in module_names)
    @test y == 2

    source_order_names = names(ConditionalExportSourceOrder7959)
    @test :future_export_7959 in source_order_names
    @test !(:leaked_future_export_7959 in source_order_names)
    @test future_export_7959 == 10
    @test ConditionalExportSourceOrder7959.leaked_future_export_7959 == 20
end

true
