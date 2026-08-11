//! Ecosystem discovery and preparation contracts for ReproCut.

mod discovery;
mod manifests;

pub use discovery::{Adapter, AdapterCommand, AdapterError, Ecosystem, EcosystemSelection};
pub use manifests::{
    CargoManifest, ManifestCapability, ManifestCategory, ManifestEntry, ManifestError, NpmManifest,
    PreparationPlan, PythonManifest,
};
pub use reprocut_workspace::InventoryPolicy;
