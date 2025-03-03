use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Write transaction failure")]
    Database(#[from] redb::Error),

    #[error("Transaction error")]
    TransactionError(#[from] redb::TransactionError),

    #[error("Commit error")]
    CommitError(#[from] redb::CommitError),

    #[error("Storage error")]
    StorageError(#[from] redb::StorageError),

    #[error("Table error")]
    TableError(#[from] redb::TableError),
}
