//! Ecosystem discovery and preparation contracts for ReproCut.

mod discovery;

pub use discovery::{Adapter, AdapterCommand, AdapterError, Ecosystem, EcosystemSelection};
pub use reprocut_workspace::InventoryPolicy;
