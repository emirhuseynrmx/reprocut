pub(crate) const SCHEMA_VERSION: i64 = reprocut_core::SESSION_SCHEMA as i64;

pub(crate) const MIGRATION_0001: &str = include_str!("../migrations/0001.sql");
pub(crate) const MIGRATION_0002: &str = include_str!("../migrations/0002.sql");
pub(crate) const MIGRATION_0003: &str = include_str!("../migrations/0003.sql");
