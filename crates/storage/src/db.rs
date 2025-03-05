use crate::{errors::StoreError, redb_impl::ReamDB};

pub struct Database {
    /// Database for the ream client
    ///
    /// Currently this implementation has a single reference to redb instance
    /// but eventually we would like to add support for multiple backends and
    /// can be set behind feature flags
    ///
    /// `redb::Database` wrapper around with async security features
    pub db: ReamDB,

    pub version: String,
}

/// This implemenation houses all the client<>db interaction logic
impl Database {
    pub fn new() -> Result<Self, StoreError> {
        Ok(Self {
            db: ReamDB::new()?,
            version: String::from("0.0.1"),
        })
    }
}
