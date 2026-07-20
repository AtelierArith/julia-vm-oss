# Function bodies execute in the top-level call site's source world.

using Test

g9650_nested(x) = x + 1
h9650_nested() = g9650_nested(1)

first = h9650_nested()

g9650_nested(x) = x + 100

second = h9650_nested()

@test first == 2
@test second == 101

true
