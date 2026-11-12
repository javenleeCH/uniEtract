use thiserror::Error;

use crate::model::Origin;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("query error: {0}")]
    Query(String),

    #[error("{origin}: stage `{stage}`: {message}")]
    Eval {
        origin: Origin,
        stage: String,
        message: String,
    },

    #[error("{0}")]
    Runtime(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl AppError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Query(_) => 2,
            _ => 1,
        }
    }

    pub fn is_broken_pipe(&self) -> bool {
        matches!(self, Self::Io(error) if error.kind() == std::io::ErrorKind::BrokenPipe)
    }

    pub fn eval(origin: &Origin, stage: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Eval {
            origin: origin.clone(),
            stage: stage.into(),
            message: message.into(),
        }
    }
}
