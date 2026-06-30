using Distributions

tol = 1e-6
ok = true

# Cauchy(0, 1)
c = Cauchy(0.0, 1.0)
ok = ok && (abs(median(c) - 0.0) < tol)
ok = ok && (abs(mode(c) - 0.0) < tol)
ok = ok && (abs(pdf(c, 0.0) - 0.3183098861837907) < tol)
ok = ok && (abs(cdf(c, 0.0) - 0.5) < tol)
ok = ok && (abs(cdf(c, 1.0) - 0.75) < tol)
ok = ok && (abs(quantile(c, 0.75) - 1.0) < tol)
ok = ok && (abs(entropy(c) - 2.5310242469692907) < tol)

# LogNormal(0, 1)
l = LogNormal(0.0, 1.0)
ok = ok && (abs(mean(l) - 1.6487212707001282) < tol)
ok = ok && (abs(median(l) - 1.0) < tol)
ok = ok && (abs(mode(l) - 0.36787944117144233) < tol)
ok = ok && (abs(var(l) - 4.670774270471604) < tol)
ok = ok && (abs(pdf(l, 1.0) - 0.3989422804014327) < tol)
ok = ok && (abs(cdf(l, 1.0) - 0.5) < tol)
ok = ok && (minimum(l) == 0.0)

# Weibull(2, 1) [shape, scale]
w = Weibull(2.0, 1.0)
ok = ok && (abs(mean(w) - 0.8862269254527580) < tol)
ok = ok && (abs(median(w) - 0.8325546111576977) < tol)
ok = ok && (abs(var(w) - 0.21460183660255172) < tol)
ok = ok && (abs(pdf(w, 1.0) - 0.7357588823428847) < tol)
ok = ok && (abs(cdf(w, 1.0) - 0.6321205588285577) < tol)
ok = ok && (abs(quantile(w, 0.6321205588285577) - 1.0) < tol)

ok
