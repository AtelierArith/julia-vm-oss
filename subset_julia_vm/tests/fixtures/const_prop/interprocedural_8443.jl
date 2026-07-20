using Test

const_prop_addone_8443(x) = x + 1
const_prop_pair_8443() = (1, 2)

@test const_prop_addone_8443(41) == 42
@test const_prop_pair_8443() == (1, 2)
@test typeof(1) === Int64

true
