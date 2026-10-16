//! # rustit-mail
//!
//! Outgoing email: message rendering and delivery transports.
//!
//! * [`messages`] — every email Rustit sends, rendered from Askama templates kept in this
//!   crate (`templates/`), each with a plain-text and an HTML part.
//! * [`Mailer`] — the delivery seam, with an SMTP transport ([`SmtpMailer`], lettre over
//!   rustls), a log transport for development ([`LogMailer`]), a transport that refuses
//!   everything ([`DisabledMailer`]) and an in-memory one for tests ([`MemoryMailer`]).
//!
//! The crate knows nothing about accounts or tokens: callers decide *what* to say and
//! *to whom*; this crate turns that into a MIME message and delivers it.

mod error;
pub mod messages;
mod transport;

pub use error::MailError;
pub use messages::{Mail, MailMessage};
pub use transport::{DisabledMailer, LogMailer, Mailer, MemoryMailer, SendFuture, Sender, SmtpMailer, from_config};
