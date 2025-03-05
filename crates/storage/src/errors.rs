use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Database error {0}")]
    Database(#[from] redb::Error),

    #[error("Transaction error {0}")]
    TransactionError(#[from] redb::TransactionError),

    #[error("Commit error {0}")]
    CommitError(#[from] redb::CommitError),

    #[error("Storage error {0}")]
    StorageError(#[from] redb::StorageError),

    #[error("Table error {0}")]
    TableError(#[from] redb::TableError),

    #[error("Io error in creating DB file {0}")]
    Io(#[from] std::io::Error),
}
