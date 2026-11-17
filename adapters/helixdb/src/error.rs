#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    FenceRejected(&'static str),
    #[error("invalid Helix configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("Helix writer contention")]
    Contention,
    #[error("Helix index work is saturated; request had no effect")]
    Backpressure,
    #[error("Helix storage operation failed")]
    Storage,
    #[error("Helix operation outcome unknown: {operation_id}")]
    OutcomeUnknown { operation_id: String },
    #[error("Helix cleanup is unconfirmed; writer remains owned")]
    CleanupUnconfirmed,
    #[error("Helix cleanup unconfirmed after: {cause}")]
    CleanupAfterFailure {
        #[source]
        cause: Box<Error>,
    },
    #[error("Helix store marker is incompatible")]
    IncompatibleSchema,
    #[error("Helix read-only handle cannot mutate storage")]
    ReadOnly,
}
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn outcome_unknown_operation(&self) -> Option<&str> {
        match self {
            Self::OutcomeUnknown { operation_id } => Some(operation_id),
            Self::CleanupAfterFailure { cause } => cause.outcome_unknown_operation(),
            _ => None,
        }
    }
}
