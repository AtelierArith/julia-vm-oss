# Issue #10194: `x = begin ... struct Foo ... end ... end` (a struct nested
# inside a `begin...end` used as an assignment's right-hand side) must lower
# and run. This exercises a dedicated identifier-assignment/begin-RHS
# lowering path (Issue #7617) distinct from the general `begin...end`
# expression path, so it needs its own coverage.

x = begin
    struct FooBeginAssignRhs10194
        y::Int
    end
    FooBeginAssignRhs10194(3).y
end

# A bare top-level `begin ... end` *statement* (not used as an expression)
# containing a nested struct must also work.
begin
    struct FooBeginStmt10194
        z::Int
    end
end

x == 3 && FooBeginStmt10194(4).z == 4
