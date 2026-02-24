# Test location macros: @__LINE__, @__FILE__, @__MODULE__

# =============================================================================
# @__LINE__ - Returns the current line number
# =============================================================================

# Test basic @__LINE__ usage
line1 = @__LINE__
println("line1: ", line1)
# Line numbers are 1-based, @__LINE__ returns the line where the macro appears

line2 = @__LINE__
println("line2: ", line2)
# Verify line2 > line1 (line numbers increase)
@assert line2 > line1

# Can be used in expressions
function report_line()
    println("Called from line: ", @__LINE__)
    return @__LINE__
end
line3 = report_line()
println("line3: ", line3)

# =============================================================================
# @__FILE__ - Returns the current file name
# =============================================================================

file = @__FILE__
println("file: ", file)
# Currently returns "<unknown>" as file tracking is not yet implemented

# =============================================================================
# @__MODULE__ - Returns the current module
# =============================================================================

mod = @__MODULE__
println("module: ", mod)
# Currently returns Main as module system is simplified

println("All location macro tests passed!")
