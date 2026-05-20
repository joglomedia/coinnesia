#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureEvent {
    BullishBos,
    BearishBos,
    BullishChoch,
    BearishChoch,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructureState {
    pub event: StructureEvent,
    pub score: f64,
}
