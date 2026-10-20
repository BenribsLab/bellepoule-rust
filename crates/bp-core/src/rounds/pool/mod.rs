//! À porter : voir ARCHITECTURE.md (poules : pool, pool_allocator, dispatcher, neo_swapper, smart_swapper, match, score).

use crate::stage::{RawStage, Stage, StageKind};

/// Fabrique des phases de ce module (signature fixe, appelée par `rounds::create_stage`).
pub fn new_stage(kind: StageKind, id: u32) -> Box<dyn Stage> {
    Box::new(RawStage::new(kind, id))
}
