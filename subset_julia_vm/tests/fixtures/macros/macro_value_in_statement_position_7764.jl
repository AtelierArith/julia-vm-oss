# Issue #7764: a value-producing macro expanded in statement position (as the
# final top-level statement) must yield its result as the program value. Before
# the fix the expanded block lowered to a Stmt::Block whose final value was
# discarded, so the program returned `nothing` / a wrong fallback instead of the
# macro's value.
macro myshow(ex)
    quote
        local result = $(esc(ex))
        println(result)
        result
    end
end

f(x) = 2x + 1

# The macro is the program's final top-level statement; its value (7) must be the
# program result. Routes through expand_macro_to_stmt.
@myshow f(3)
