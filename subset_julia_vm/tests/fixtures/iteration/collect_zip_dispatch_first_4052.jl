using Base.Iterators

import Base: collect

collect(z::Base.Iterators.Zip) = :zip_dispatch

values = collect(zip([1, 2], [3, 4]))
@assert values === :zip_dispatch

true
