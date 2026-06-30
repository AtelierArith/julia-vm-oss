using StatsBase

h = fit(Histogram, [1.0, 2.0, 2.0, 3.0, 3.0, 3.0]; nbins=3)
sum(h.weights) == 6
