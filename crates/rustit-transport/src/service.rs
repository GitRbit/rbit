//! The two Git services a client can request.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// Fetch and clone (read).
    UploadPack,
    /// Push (write).
    ReceivePack,
}

impl Service {
    /// Parses the `service` query parameter / URL segment (`git-upload-pack`, `git-receive-pack`).
    pub fn from_wire_name(name: &str) -> Option<Self> {
        match name {
            "git-upload-pack" => Some(Self::UploadPack),
            "git-receive-pack" => Some(Self::ReceivePack),
            _ => None,
        }
    }

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::UploadPack => "git-upload-pack",
            Self::ReceivePack => "git-receive-pack",
        }
    }

    /// The `git` sub-command implementing this service.
    pub const fn subcommand(self) -> &'static str {
        match self {
            Self::UploadPack => "upload-pack",
            Self::ReceivePack => "receive-pack",
        }
    }

    /// Whether the service modifies the repository and therefore needs write access.
    pub const fn is_write(self) -> bool {
        matches!(self, Self::ReceivePack)
    }

    pub fn advertisement_content_type(self) -> String {
        format!("application/x-{}-advertisement", self.wire_name())
    }

    pub fn result_content_type(self) -> String {
        format!("application/x-{}-result", self.wire_name())
    }
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire_name())
    }
}
