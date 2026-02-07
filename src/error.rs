use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContactsError {
    #[error("contacts access denied")]
    AccessDenied,
    #[error("contact not found: {0}")]
    ContactNotFound(String),
    #[error("group not found: {0}")]
    GroupNotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("operation failed: {0}")]
    OperationFailed(String),
    #[error("objective-c error: {0}")]
    ObjcError(String),
    #[error("internal channel error: {0}")]
    ChannelError(String),
}
