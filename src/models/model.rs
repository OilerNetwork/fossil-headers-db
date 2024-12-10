use core::error;
use std::fmt::Display;

#[derive(Debug)]
pub enum ModelError {
    DatabaseError(sqlx::Error),
    UpdateError(String),
    UnexpectedError,
}

impl Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseError(err) => err.fmt(f),
            Self::UpdateError(err_str) => err_str.fmt(f),
            Self::UnexpectedError => write!(f, "Unexpected model error"),
        }
    }
}

impl error::Error for ModelError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            ModelError::DatabaseError(err) => Some(err),
            ModelError::UpdateError(_) => None,
            ModelError::UnexpectedError => None,
        }
    }
}

impl From<sqlx::Error> for ModelError {
    fn from(err: sqlx::Error) -> Self {
        ModelError::DatabaseError(err)
    }
}
