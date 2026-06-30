using Test

module QualifiedInnerConstructor7631
export Widget

struct Widget
    value
    Widget() = new(42)
end
end

@testset "Issue #7631: qualified inner constructor" begin
    w = QualifiedInnerConstructor7631.Widget()
    @test w.value == 42
end

true
