/// Complex number representation for matrix operations.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Complex64 {
    pub(crate) re: f64,
    pub(crate) im: f64,
}

impl Complex64 {
    pub(crate) fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    pub(crate) fn from_real(re: f64) -> Self {
        Self { re, im: 0.0 }
    }

    pub(crate) fn add(self, other: Self) -> Self {
        Self {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }

    pub(crate) fn mul(self, other: Self) -> Self {
        // (a + bi)(c + di) = (ac - bd) + (ad + bc)i
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Complex64::new / from_real ─────────────────────────────────────────────

    #[test]
    fn test_complex64_new_stores_re_im() {
        let c = Complex64::new(3.0, 4.0);
        assert!((c.re - 3.0).abs() < 1e-15);
        assert!((c.im - 4.0).abs() < 1e-15);
    }

    #[test]
    fn test_complex64_from_real_has_zero_imaginary() {
        let c = Complex64::from_real(7.0);
        assert!((c.re - 7.0).abs() < 1e-15);
        assert!(c.im.abs() < 1e-15, "imaginary part must be 0.0, got {}", c.im);
    }

    // ── Complex64::add ────────────────────────────────────────────────────────

    #[test]
    fn test_complex64_add_component_wise() {
        // (1 + 2i) + (3 + 4i) = 4 + 6i
        let a = Complex64::new(1.0, 2.0);
        let b = Complex64::new(3.0, 4.0);
        let result = a.add(b);
        assert!((result.re - 4.0).abs() < 1e-15);
        assert!((result.im - 6.0).abs() < 1e-15);
    }

    #[test]
    fn test_complex64_add_with_zero() {
        // (5 + 3i) + (0 + 0i) = 5 + 3i
        let a = Complex64::new(5.0, 3.0);
        let zero = Complex64::new(0.0, 0.0);
        let result = a.add(zero);
        assert!((result.re - 5.0).abs() < 1e-15);
        assert!((result.im - 3.0).abs() < 1e-15);
    }

    // ── Complex64::mul ────────────────────────────────────────────────────────

    #[test]
    fn test_complex64_mul_standard() {
        // (1 + 2i)(3 + 4i) = (3 - 8) + (4 + 6)i = -5 + 10i
        let a = Complex64::new(1.0, 2.0);
        let b = Complex64::new(3.0, 4.0);
        let result = a.mul(b);
        assert!((result.re - (-5.0)).abs() < 1e-10);
        assert!((result.im - 10.0).abs() < 1e-10);
    }

    #[test]
    fn test_complex64_mul_pure_imaginary_squared_is_minus_one() {
        // i * i = -1: (0 + 1i) * (0 + 1i) = (0*0 - 1*1) + (0*1 + 1*0)i = -1 + 0i
        let i = Complex64::new(0.0, 1.0);
        let result = i.mul(i);
        assert!((result.re - (-1.0)).abs() < 1e-15, "i² should be -1, got {}", result.re);
        assert!(result.im.abs() < 1e-15, "imaginary part should be 0, got {}", result.im);
    }

    #[test]
    fn test_complex64_mul_by_real_scalar() {
        // (2 + 3i) * 4 = 8 + 12i
        let a = Complex64::new(2.0, 3.0);
        let real = Complex64::from_real(4.0);
        let result = a.mul(real);
        assert!((result.re - 8.0).abs() < 1e-15);
        assert!((result.im - 12.0).abs() < 1e-15);
    }
}
