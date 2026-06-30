using Test
using Distributions

close(a, b, atol) = abs(a - b) < atol

@testset "Distributions milestone parity #7332" begin
    @test close(pdf(Normal(), 0.0), 0.3989422804014327, 1e-8)
    @test close(cdf(Gamma(2.0, 3.0), 4.0), 0.3849400110637805, 1e-6)
    @test close(quantile(Beta(2.0, 5.0), 0.5), 0.26444998329565994, 1e-5)

    @test close(pdf(TDist(5.0), 1.0), 0.21967979735098056, 1e-6)
    @test close(cdf(Chisq(4.0), 1.0), 0.09020401043104986, 1e-6)
    @test close(quantile(FDist(5.0, 10.0), 0.5), 0.9319331608510479, 1e-5)

    @test close(pdf(Laplace(1.0, 2.0), 0.0), 0.15163266492815836, 1e-6)
    @test close(cdf(Logistic(1.0, 2.0), 1.0), 0.5, 1e-8)
    @test close(pdf(Rayleigh(2.0), 2.0), 0.3032653298563167, 1e-6)
    @test close(cdf(Pareto(5.0, 2.0), 3.0), 0.8683127572016461, 1e-6)
    @test close(pdf(Gumbel(1.0, 2.0), 1.0), 0.18393972058572117, 1e-6)
    @test close(cdf(Frechet(5.0, 2.0), 2.0), 0.36787944117144233, 1e-6)
    @test close(quantile(Levy(0.0, 1.0), 0.5), 2.198109338317732, 1e-6)

    @test close(pdf(Chi(4.0), 1.5), 0.5478510386672151, 1e-6)
    @test close(cdf(Erlang(3, 2.0), 4.0), 0.3233235838169365, 1e-6)
    @test close(pdf(InverseGamma(4.0, 2.0), 0.75), 0.7808071775270097, 1e-6)
    @test close(cdf(InverseGaussian(1.5, 3.0), 1.0), 0.3881108178559103, 1e-6)
    @test close(pdf(Arcsine(0.0, 2.0), 0.5), 0.3675525969478614, 1e-6)
    @test close(cdf(TriangularDist(0.0, 4.0, 1.0), 1.0), 0.25, 1e-8)
    @test close(pdf(SymTriangularDist(1.0, 2.0), 1.5), 0.375, 1e-8)
    @test close(cdf(Cosine(0.0, 2.0), 0.5), 0.7375395395196382, 1e-6)
    @test close(pdf(Semicircle(2.0), 1.0), 0.27566444771089604, 1e-6)
    @test close(cdf(Kumaraswamy(2.0, 3.0), 0.5), 0.578125, 1e-8)

    @test close(pdf(NegativeBinomial(5.0, 0.4), 3), 0.07741440000000026, 1e-6)
    @test close(cdf(Hypergeometric(30, 20, 10), 6), 0.6350317132231632, 1e-6)
    @test close(pdf(BetaBinomial(12, 2.0, 5.0), 3), 0.15406162464986023, 1e-6)
    @test close(pdf(Skellam(4.0, 1.5), -2), 0.02427201604814087, 1e-5)
    @test close(cdf(Skellam(4.0, 1.5), 3), 0.6787966150124363, 1e-5)
    @test pdf(Dirac(3), 3) == 1.0
    @test close(cdf(PoissonBinomial([0.2, 0.5, 0.8]), 2), 0.9199999999999999, 1e-8)

    tn = truncated(Normal(), -1.0, 1.0)
    @test minimum(tn) == -1.0
    @test maximum(tn) == 1.0
    @test close(quantile(tn, 0.5), 0.0, 1e-8)

    fitted = fit_mle(Normal, [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0])
    @test close(mean(fitted), 5.0, 1e-8)
    @test close(std(fitted), 2.0, 1e-8)
end

true
