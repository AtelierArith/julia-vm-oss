# Test baremodule syntax (Julia 1.0+)
# baremodule is like module but doesn't automatically import Base
# Note: In a true baremodule, even + is not available without explicit imports
# This test verifies basic baremodule parsing and function definition

baremodule SimpleFunctions
    # Functions that only use Core operations (no Base needed)
    function echo_value(x)
        return x
    end

    function get_fortytwo()
        return 42
    end

    export echo_value, get_fortytwo
end

using .SimpleFunctions

# Test exported functions
@assert echo_value(10) == 10
@assert get_fortytwo() == 42

# Test module-qualified access
@assert SimpleFunctions.echo_value(20) == 20
@assert SimpleFunctions.get_fortytwo() == 42

# All tests passed
true
