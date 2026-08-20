use super::super::types::StaticType;

/// Constant value.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Int64(i64),
    Int32(i32),
    Float64(f64),
    Float32(f32),
    Bool(bool),
    Char(char),
    String(String),
    Nothing,
}

impl ConstValue {
    /// Get the type of this constant.
    pub fn get_type(&self) -> StaticType {
        match self {
            ConstValue::Int64(_) => StaticType::I64,
            ConstValue::Int32(_) => StaticType::I32,
            ConstValue::Float64(_) => StaticType::F64,
            ConstValue::Float32(_) => StaticType::F32,
            ConstValue::Bool(_) => StaticType::Bool,
            ConstValue::Char(_) => StaticType::Char,
            ConstValue::String(_) => StaticType::Str,
            ConstValue::Nothing => StaticType::Nothing,
        }
    }
}
