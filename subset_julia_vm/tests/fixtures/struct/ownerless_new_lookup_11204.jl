using Test

function missing_ownerless_new11204()
    new(1)
end

function missing_ownerless_lambda11204()
    (() -> new(2))()
end

@test_throws UndefVarError missing_ownerless_new11204()
@test_throws UndefVarError missing_ownerless_lambda11204()

# Keep the missing-binding case in a source with no later `new` definition.
# First-binding source visibility is a separate compiler issue (Issue #11210).

true
