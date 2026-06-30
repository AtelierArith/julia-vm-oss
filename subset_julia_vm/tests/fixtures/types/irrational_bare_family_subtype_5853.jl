using Test

@test Irrational{:π} <: Irrational
@test Irrational{:ℯ} <: Irrational
@test !(Irrational{:π} <: Rational)

true
