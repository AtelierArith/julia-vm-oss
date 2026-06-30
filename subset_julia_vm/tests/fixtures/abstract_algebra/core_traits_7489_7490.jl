using AbstractAlgebra

zz_parent = parent(3)
qq_parent = parent(2//3)

println(AbstractAlgebra.ZZ)
println(AbstractAlgebra.QQ)
println(zz_parent)
println(qq_parent)

ok = AbstractAlgebra.ZZ === ZZ &&
     AbstractAlgebra.QQ === QQ &&
     parent(big(3)) === ZZ &&
     parent(big(2)//big(3)) === QQ &&
     elem_type(ZZ) === BigInt &&
     elem_type(QQ) === Rational{BigInt} &&
     parent_type(BigInt) === typeof(ZZ) &&
     parent_type(Rational{BigInt}) === typeof(QQ) &&
     base_ring_type(typeof(ZZ)) === Union{} &&
     base_ring(QQ) === ZZ &&
     base_ring_type(typeof(QQ)) === typeof(ZZ) &&
     is_exact_type(BigInt) &&
     is_exact_type(Rational{BigInt}) &&
     is_domain_type(BigInt) &&
     is_domain_type(Rational{BigInt}) &&
     characteristic(ZZ) == 0 &&
     characteristic(QQ) == 0 &&
     is_known(characteristic, ZZ) &&
     is_known(characteristic, QQ) &&
     check_parent(big(2), big(5)) &&
     !check_parent(big(2), big(2)//big(3), false) &&
     zero(ZZ) == big(0) &&
     one(ZZ) == big(1) &&
     zero(QQ) == big(0)//big(1) &&
     one(QQ) == big(1)//big(1) &&
     is_unit(big(-1)) &&
     !is_unit(big(2)) &&
     canonical_unit(big(-9)) == big(-1) &&
     is_zero_divisor(big(0)) &&
     !is_zero_divisor(big(4)) &&
     divides(big(12), big(-3)) == (true, big(-4)) &&
     divides(big(12), big(5))[1] == false &&
     is_divisible_by(big(-12), big(3)) &&
     !is_divisible_by(big(12), big(5)) &&
     divexact(big(-12), big(3)) == big(-4) &&
     numerator(big(6)//big(8)) == big(3) &&
     denominator(big(6)//big(8)) == big(4) &&
     divexact(big(3)//big(4), big(3)//big(2)) == big(1)//big(2) &&
     sqrt(big(49)) == big(7) &&
     is_square(big(49)) &&
     !is_square(big(50)) &&
     root(big(27), 3) == big(3) &&
     AbstractAlgebra.parent(big(7)) === ZZ &&
     AbstractAlgebra.elem_type(AbstractAlgebra.QQ) === Rational{BigInt} &&
     AbstractAlgebra.divexact(big(18), big(6)) == big(3)

err = sprint(showerror, AbstractAlgebra.NotImplementedError(:demo, ZZ, QQ))
# #8256 is fixed: the package-defined `Base.showerror(io, ::NotImplementedError)`
# now dispatches, producing upstream AbstractAlgebra's message form. That message
# describes the call ("function <head> is not implemented for arguments ...") and
# does NOT echo the exception type name, so assert against the message text
# rather than the literal "NotImplementedError" (Issue #8273).
ok = ok && occursin("function demo is not implemented", err)

println(ok)
ok
