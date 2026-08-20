use super::super::types::StaticType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateLayout {
    pub id: u32,
    pub size: u32,
    pub align: u8,
    pub fields: Vec<AggregateField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateField {
    pub offset: u32,
    pub ty: StaticType,
    pub layout_id: u32,
}
