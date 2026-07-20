# Issue #10370: a user-defined macro whose `quote ... end` body directly
# contains a literal `struct Name ... end` definition (not via
# `$()`-interpolation of a caller argument — contrast with
# `user_macro_quote_nested_struct_10194.jl`) must lower AND be callable at
# runtime. This exercises the user-defined-macro VM-execution path
# (`macro_runtime.rs`'s `evaluate_macro` -> `value_to_stmt`'s
# `ExprHead::Struct` arm), which queues the struct via
# `add_macro_expanded_struct` exactly like Issue #10194's
# `lower_transparent_block_stmts`. Previously this lowered without error but
# the constructor was not callable — `Unknown function: FooUserMacro10194`
# at runtime — because `LoweringWithInclude::lower` (the file/CLI lowering
# entry point) never drained the macro-expanded-struct queue. Fixed as a
# side effect of Issue #10194's drain fix (same root cause).

macro make_struct_10194()
    quote
        struct FooUserMacro10194
            x::Int
        end
    end
end

@make_struct_10194()

FooUserMacro10194(1).x == 1
