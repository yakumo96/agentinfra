use std::fmt;

use crate::ErrorCode;

/// A tool-level error. Rendered by the MCP layer as `isError: true` with
/// `structuredContent = {code, message, retryable, details}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfraError {
    pub code: ErrorCode,
    pub message: String,
    /// Paths the error is about (`details.paths`), e.g. the op that failed validation.
    pub paths: Vec<String>,
    /// For `OPERATION_IN_PROGRESS`: the operation the caller should wait for.
    pub operation_id: Option<String>,
}

pub type InfraResult<T> = Result<T, InfraError>;

impl InfraError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            paths: Vec::new(),
            operation_id: None,
        }
    }

    pub fn at(mut self, path: impl Into<String>) -> Self {
        self.paths.push(path.into());
        self
    }

    pub fn with_operation(mut self, operation_id: impl Into<String>) -> Self {
        self.operation_id = Some(operation_id.into());
        self
    }

    pub fn retryable(&self) -> bool {
        self.code.retryable()
    }
}

impl fmt::Display for InfraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)?;
        if !self.paths.is_empty() {
            write!(f, " ({})", self.paths.join(", "))?;
        }
        Ok(())
    }
}

impl std::error::Error for InfraError {}
