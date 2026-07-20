//! Shared Julia string code-unit index validation.

use crate::vm::{value::decode_julia_char, VmError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::vm) enum StringIndexValidation {
    Character { byte_start: usize, byte_end: usize },
    OutOfBounds,
    NonCharacterBoundary { valid_indices: (i64, i64) },
}

/// Classify a one-based Julia string index without constructing an exception.
///
/// Malformed UTF-8 follows `decode_julia_char` segmentation: continuation bytes
/// consumed by the preceding character are invalid indices, while a standalone
/// continuation byte is itself a malformed character start.
pub(in crate::vm) fn validate_string_index(bytes: &[u8], index: i64) -> StringIndexValidation {
    if index < 1 || index as u64 > bytes.len() as u64 {
        return StringIndexValidation::OutOfBounds;
    }

    let target = (index - 1) as usize;
    let mut byte_start = 0usize;
    while byte_start < bytes.len() {
        let (_, byte_end) = decode_julia_char(bytes, byte_start);
        if byte_start == target {
            return StringIndexValidation::Character {
                byte_start,
                byte_end,
            };
        }
        if byte_end > target {
            return StringIndexValidation::NonCharacterBoundary {
                valid_indices: (
                    (byte_start + 1) as i64,
                    if byte_end < bytes.len() {
                        (byte_end + 1) as i64
                    } else {
                        -1
                    },
                ),
            };
        }
        byte_start = byte_end;
    }

    StringIndexValidation::OutOfBounds
}

pub(super) fn string_char_byte_span(bytes: &[u8], index: i64) -> Result<(usize, usize), VmError> {
    match validate_string_index(bytes, index) {
        StringIndexValidation::Character {
            byte_start,
            byte_end,
        } => Ok((byte_start, byte_end)),
        StringIndexValidation::OutOfBounds => Err(VmError::IndexOutOfBounds {
            indices: vec![index],
            shape: vec![bytes.len()],
        }),
        StringIndexValidation::NonCharacterBoundary { valid_indices } => {
            Err(VmError::StringIndexError {
                index,
                valid_indices,
            })
        }
    }
}
