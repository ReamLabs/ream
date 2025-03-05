use crate::redb_interface::ReamDB;
use crate::errors::StoreError;

pub struct Database{
    /// Database for the ream client 
    /// 
    /// Currently this implementation has a single reference to redb instance
    /// but eventually we would like to add support for multiple backends and 
    /// can be set behind feature flags
    ///
    /// `redb::Database` wrapper around with async security features
    db: ReamDB,
    
    pub version: String 
}

/// 
/// This implemenation houses all the client<>db interaction logic
///
impl Database {
    pub fn new() -> Result<Self, StoreError> {
        let db_instance = ReamDB::new()?; 
        Ok(Self{
            db: db_instance,
            version: String::from("0.0.1")
        }) 
    }
}
