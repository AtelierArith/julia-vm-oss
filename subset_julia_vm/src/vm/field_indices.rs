//! Named indices for well-known struct fields and array dimensions.
//!
//! These constants remove magic numbers when reading semantic fields
//! (e.g. complex real/imag parts, rational numerator/denominator).

pub(crate) const FIRST_FIELD_INDEX: usize = 0;
pub(crate) const SECOND_FIELD_INDEX: usize = 1;
pub(crate) const THIRD_FIELD_INDEX: usize = 2;
pub(crate) const FOURTH_FIELD_INDEX: usize = 3;

pub(crate) const COMPLEX_REAL_FIELD_INDEX: usize = FIRST_FIELD_INDEX;
pub(crate) const COMPLEX_IMAG_FIELD_INDEX: usize = SECOND_FIELD_INDEX;
pub(crate) const RATIONAL_NUMERATOR_FIELD_INDEX: usize = FIRST_FIELD_INDEX;
pub(crate) const RATIONAL_DENOMINATOR_FIELD_INDEX: usize = SECOND_FIELD_INDEX;

pub(crate) const ARRAY_FIRST_DIM_INDEX: usize = FIRST_FIELD_INDEX;
pub(crate) const ARRAY_SECOND_DIM_INDEX: usize = SECOND_FIELD_INDEX;
