using Test

# Issue #3622: typed-parameter narrow integer widths previously dropped to
# Int64 between parameter binding and arithmetic dispatch. The cause was
# `infer_expr_type(Var(name))` consulting `compiler.locals` (which collapses
# every narrow integer type to ValueType::I64) instead of `julia_type_locals`
# (which records the precise JuliaType for narrow-integer params). After the
# fix in compile/expr/infer/mod.rs, same-width arithmetic on typed params
# preserves the declared width across +, -, *, ÷, %.

@testset "Typed-parameter UInt arithmetic preserves width (Issue #3622)" begin
    function add_u8(a::UInt8, b::UInt8); a + b end
    function sub_u8(a::UInt8, b::UInt8); a - b end
    function mul_u8(a::UInt8, b::UInt8); a * b end
    function div_u8(a::UInt8, b::UInt8); a ÷ b end
    function mod_u8(a::UInt8, b::UInt8); a % b end

    @test typeof(add_u8(UInt8(1), UInt8(2))) === UInt8
    @test typeof(sub_u8(UInt8(5), UInt8(2))) === UInt8
    @test typeof(mul_u8(UInt8(3), UInt8(2))) === UInt8
    @test typeof(div_u8(UInt8(10), UInt8(3))) === UInt8
    @test typeof(mod_u8(UInt8(10), UInt8(3))) === UInt8
    @test add_u8(UInt8(1), UInt8(2)) == UInt8(3)
    @test mul_u8(UInt8(3), UInt8(2)) == UInt8(6)

    function add_u16(a::UInt16, b::UInt16); a + b end
    function add_u32(a::UInt32, b::UInt32); a + b end
    function add_u64(a::UInt64, b::UInt64); a + b end
    function add_u128(a::UInt128, b::UInt128); a + b end

    @test typeof(add_u16(UInt16(1), UInt16(2))) === UInt16
    @test typeof(add_u32(UInt32(1), UInt32(2))) === UInt32
    @test typeof(add_u64(UInt64(1), UInt64(2))) === UInt64
    @test typeof(add_u128(UInt128(1), UInt128(2))) === UInt128
end

@testset "Typed-parameter signed-narrow arithmetic preserves width (Issue #3622)" begin
    function add_i8(a::Int8, b::Int8); a + b end
    function sub_i8(a::Int8, b::Int8); a - b end
    function mul_i8(a::Int8, b::Int8); a * b end
    function div_i8(a::Int8, b::Int8); a ÷ b end
    function mod_i8(a::Int8, b::Int8); a % b end

    @test typeof(add_i8(Int8(1), Int8(2))) === Int8
    @test typeof(sub_i8(Int8(5), Int8(2))) === Int8
    @test typeof(mul_i8(Int8(3), Int8(2))) === Int8
    @test typeof(div_i8(Int8(10), Int8(3))) === Int8
    @test typeof(mod_i8(Int8(10), Int8(3))) === Int8

    function add_i16(a::Int16, b::Int16); a + b end
    function add_i32(a::Int32, b::Int32); a + b end
    function add_i128(a::Int128, b::Int128); a + b end
    function sub_i128(a::Int128, b::Int128); a - b end
    function mul_i128(a::Int128, b::Int128); a * b end
    function div_i128(a::Int128, b::Int128); a ÷ b end
    function mod_i128(a::Int128, b::Int128); a % b end

    @test typeof(add_i16(Int16(1), Int16(2))) === Int16
    @test typeof(add_i32(Int32(1), Int32(2))) === Int32
    @test typeof(add_i128(Int128(1), Int128(2))) === Int128
    @test typeof(sub_i128(Int128(5), Int128(2))) === Int128
    @test typeof(mul_i128(Int128(3), Int128(2))) === Int128
    @test typeof(div_i128(Int128(10), Int128(3))) === Int128
    @test typeof(mod_i128(Int128(10), Int128(3))) === Int128
end

@testset "Typed-parameter Int64/Float64 regression guards" begin
    function add_i64(a::Int64, b::Int64); a + b end
    function add_f64(a::Float64, b::Float64); a + b end

    @test typeof(add_i64(1, 2)) === Int64
    @test typeof(add_f64(1.0, 2.0)) === Float64
end

true
