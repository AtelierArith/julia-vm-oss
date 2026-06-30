macro sees_tuple_arg(ex)
    ok = ex isa Expr &&
         ex.head == :tuple &&
         length(ex.args) == 2 &&
         ex.args[1] == :alpha &&
         ex.args[2] == :beta
    return ok
end

@assert @sees_tuple_arg alpha, beta

# Issue #7676 tracks var-string identifier AST parity separately; this fixture
# keeps Issue #7526 focused on whitespace macro comma grouping.

true
