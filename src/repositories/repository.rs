use core::error;
use std::fmt::Display;

#[derive(Debug)]
pub enum RepositoryError {
    DatabaseError(sqlx::Error),
    UpdateError(String),
    UnexpectedError,
}

impl Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseError(err) => err.fmt(f),
            Self::UpdateError(err_str) => err_str.fmt(f),
            Self::UnexpectedError => write!(f, "Unexpected model error"),
        }
    }
}

impl error::Error for RepositoryError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            RepositoryError::DatabaseError(err) => Some(err),
            RepositoryError::UpdateError(_) => None,
            RepositoryError::UnexpectedError => None,
        }
    }
}

impl From<sqlx::Error> for RepositoryError {
    fn from(err: sqlx::Error) -> Self {
        RepositoryError::DatabaseError(err)
    }
}
