//! Why a message could not be rendered or delivered.

#[derive(Debug, thiserror::Error)]
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
