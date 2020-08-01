use std::{
    any::Any,
    collections::HashMap,
    fmt::{self, Display},
};

#[derive(Debug)]
pub enum DomainError {
    Params(Vec<ErrorDetail>),
    Internal(ErrorDetail),
    Others(Box<dyn std::error::Error>),
}

pub type DomainResult<T> = std::result::Result<T, DomainError>;

impl Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::Params(payload) => {
                for d in payload.iter() {
                    f.write_str(d.to_string().as_str())?;
                    f.write_str(", ")?;
                }
                Ok(())
            }
            DomainError::Internal(payload) => payload.fmt(f),
            DomainError::Others(payload) => payload.fmt(f),
        }
    }
}

impl std::error::Error for DomainError {}

impl DomainError {
    pub fn params(code: &'static str, message: String) -> DomainError {
        DomainError::Params(vec![ErrorDetail::new(code, message)])
    }

    pub fn internal(code: &'static str, message: String) -> DomainError {
        DomainError::Internal(ErrorDetail::new(code, message))
    }
}

impl From<Box<dyn std::error::Error>> for DomainError {
    fn from(e: Box<dyn std::error::Error>) -> Self {
        DomainError::Others(e)
    }
}

#[derive(Debug)]
pub struct ErrorDetail {
    code: &'static str,
    message: String,
    user_info: Option<HashMap<String, Box<dyn Any>>>,
}

impl Display for ErrorDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message.as_str())
    }
}

impl ErrorDetail {
    pub fn new(code: &'static str, message: String) -> ErrorDetail {
        ErrorDetail { code, message, user_info: None }
    }
}
