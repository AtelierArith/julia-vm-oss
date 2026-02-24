# =============================================================================
# util.jl - Utility functions
# =============================================================================
# Based on Julia's base/util.jl
# Provides terminal output utilities including colored/styled printing.

# =============================================================================
# ANSI Color Codes
# =============================================================================
# ESC character (ASCII 27) for ANSI escape sequences
# Note: Const values defined here cannot be accessed from function bodies
# due to SubsetJuliaVM limitations (Issue #1443). Functions generate
# escape sequences inline instead.

const _ESC = string(Char(27))
const _ANSI_RESET = string(Char(27), "[0m")
const _ANSI_BOLD = string(Char(27), "[1m")

# Pre-computed color codes (for documentation; not used in functions)
const _ANSI_BLACK = string(Char(27), "[30m")
const _ANSI_RED = string(Char(27), "[31m")
const _ANSI_GREEN = string(Char(27), "[32m")
const _ANSI_YELLOW = string(Char(27), "[33m")
const _ANSI_BLUE = string(Char(27), "[34m")
const _ANSI_MAGENTA = string(Char(27), "[35m")
const _ANSI_CYAN = string(Char(27), "[36m")
const _ANSI_WHITE = string(Char(27), "[37m")
const _ANSI_LIGHT_BLACK = string(Char(27), "[90m")
const _ANSI_LIGHT_RED = string(Char(27), "[91m")
const _ANSI_LIGHT_GREEN = string(Char(27), "[92m")
const _ANSI_LIGHT_YELLOW = string(Char(27), "[93m")
const _ANSI_LIGHT_BLUE = string(Char(27), "[94m")
const _ANSI_LIGHT_MAGENTA = string(Char(27), "[95m")
const _ANSI_LIGHT_CYAN = string(Char(27), "[96m")
const _ANSI_LIGHT_WHITE = string(Char(27), "[97m")
const _ANSI_DEFAULT = string(Char(27), "[39m")

# Helper function to get color code from symbol
# Note: Generates escape sequences inline due to global const limitation
function _get_ansi_color(color::Symbol)
    esc = Char(27)
    if color === :black
        return string(esc, "[30m")
    elseif color === :red
        return string(esc, "[31m")
    elseif color === :green
        return string(esc, "[32m")
    elseif color === :yellow
        return string(esc, "[33m")
    elseif color === :blue
        return string(esc, "[34m")
    elseif color === :magenta
        return string(esc, "[35m")
    elseif color === :cyan
        return string(esc, "[36m")
    elseif color === :white
        return string(esc, "[37m")
    elseif color === :light_black
        return string(esc, "[90m")
    elseif color === :light_red
        return string(esc, "[91m")
    elseif color === :light_green
        return string(esc, "[92m")
    elseif color === :light_yellow
        return string(esc, "[93m")
    elseif color === :light_blue
        return string(esc, "[94m")
    elseif color === :light_magenta
        return string(esc, "[95m")
    elseif color === :light_cyan
        return string(esc, "[96m")
    elseif color === :light_white
        return string(esc, "[97m")
    elseif color === :normal
        return string(esc, "[0m")
    elseif color === :default
        return string(esc, "[39m")
    else
        return ""  # Unknown color, no formatting
    end
end

# =============================================================================
# printstyled - Print with ANSI styling
# =============================================================================
# Based on Julia's base/util.jl
#
# Print text with ANSI color and style formatting.
#
# Supported colors:
#   :black, :red, :green, :yellow, :blue, :magenta, :cyan, :white
#   :light_black, :light_red, :light_green, :light_yellow,
#   :light_blue, :light_magenta, :light_cyan, :light_white
#   :normal (reset all), :default (default foreground)

# Simplified API: printstyled(text, color)
# Note: Using separate print calls due to VM limitation with multi-arg print
# where the first arg contains escape sequences
function printstyled(text, color::Symbol)
    esc = Char(27)
    code = _get_ansi_color(color)
    print(code)
    print(text)
    print(string(esc, "[0m"))  # Reset
    nothing
end

# With bold: printstyled(text, color, bold)
function printstyled(text, color::Symbol, bold::Bool)
    esc = Char(27)
    code = _get_ansi_color(color)
    if bold
        code = code * string(esc, "[1m")
    end
    print(code)
    print(text)
    print(string(esc, "[0m"))  # Reset
    nothing
end

# Ensure the file ends with nothing to avoid returning unexpected values
nothing
