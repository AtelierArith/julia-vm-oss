# Test html"..." string literal (Issue #468)
# Tests that html"text" creates an HTML{String} object

using Test

@testset "HTML string literal" begin
    # Basic html literal
    h = html"<b>bold</b>"
    @test isa(h, HTML{String})
    @test h.content == "<b>bold</b>"

    # HTML with various content
    h2 = html"<div>hello world</div>"
    @test isa(h2, HTML{String})
    @test h2.content == "<div>hello world</div>"

    # Empty HTML
    h3 = html""
    @test isa(h3, HTML{String})
    @test h3.content == ""

    # HTML equality
    @test html"test" == html"test"
    @test !(html"a" == html"b")
end

true
