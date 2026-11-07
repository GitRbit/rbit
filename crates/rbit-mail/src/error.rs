//! Why a message could not be rendered or delivered.

#[derive(thiserror::Error)]
pub enum MailError {
    /// The instance has no mail transport configured (`[mail] transport = "disabled"`).
    #[error("email is disabled on this instance")]
    Disabled,

    /// A configured address (sender or recipient) is not a valid mailbox.
    #[error("invalid email address {address:?}: {reason}")]
    Address { address: String, reason: String },

    /// The SMTP settings could not be turned into a transport (e.g. an empty host name).
    #[error("invalid SMTP configuration: {0}")]
    Config(String),

    #[error("could not render the message: {0}")]
    Render(#[from] askama::Error),

    #[error("could not build the message: {0}")]
    Build(#[from] lettre::error::Error),

    #[error("SMTP delivery failed: {0}")]
    Smtp(#[from] lettre::transport::smtp::Error),

    /// The transport did not accept the message in time.
    #[error("delivery timed out after {seconds} seconds")]
    Timeout { seconds: u64 },
}

// Display and Error::source retain the existing caller-facing diagnostic contract.
// Debug does not forward private addresses or upstream error details.
impl std::fmt::Debug for MailError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Disabled => "Disabled",
            Self::Address { .. } => "Address",
            Self::Config(_) => "Config",
            Self::Render(_) => "Render",
            Self::Build(_) => "Build",
            Self::Smtp(_) => "Smtp",
            Self::Timeout { .. } => "Timeout",
        };
        out.debug_struct(name).finish_non_exhaustive()
    }
}
