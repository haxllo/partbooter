use std::fmt::{Display, Formatter};
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppErrorKind {
    UnsupportedEnvironment,
    Validation,
    Privilege,
    Io,
    Verification,
    Rollback,
    Usage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    kind: AppErrorKind,
    message: String,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(kind: AppErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> AppErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn exit_code(&self) -> i32 {
        match self.kind {
            AppErrorKind::UnsupportedEnvironment => 2,
            AppErrorKind::Validation => 3,
            AppErrorKind::Privilege => 4,
            AppErrorKind::Io => 5,
            AppErrorKind::Verification => 6,
            AppErrorKind::Rollback => 7,
            AppErrorKind::Usage => 64,
        }
    }
}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

impl From<io::Error> for AppError {
    fn from(value: io::Error) -> Self {
        Self::new(AppErrorKind::Io, value.to_string())
    }
}
