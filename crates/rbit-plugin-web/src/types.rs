use std::{fmt::Debug, future::Future, pin::Pin, sync::Arc, time::Duration};

pub const MAX_HOOKS: usize = 16;
pub const MAX_PATH_BYTES: usize = 2048;
pub const MAX_HOOK_DEADLINE: Duration = Duration::from_millis(100);
pub const MAX_PIPELINE_DEADLINE: Duration = Duration::from_millis(500);
pub const MAX_HOOK_ANNOTATIONS: usize = 4;
pub const MAX_RESPONSE_ANNOTATIONS: usize = 16;

/// Closed protocol metadata: extension methods do not expose arbitrary user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Trace,
    Connect,
    Other,
}
impl RequestMethod {
    pub fn from_http(value: &str) -> Self {
        match value {
            "GET" => Self::Get,
            "HEAD" => Self::Head,
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "PATCH" => Self::Patch,
            "DELETE" => Self::Delete,
            "OPTIONS" => Self::Options,
            "TRACE" => Self::Trace,
            "CONNECT" => Self::Connect,
            _ => Self::Other,
        }
    }
}

/// The route is a host-declared pattern, never the raw path, query or credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestMetadata {
    method: RequestMethod,
    route: String,
}
impl RequestMetadata {
    pub fn new(method: RequestMethod, route: &str) -> Result<Self, HookRegistrationError> {
        if route.len() > MAX_PATH_BYTES || route.bytes().any(|byte| byte.is_ascii_control() || byte == b'?' || byte == b'#') {
            return Err(HookRegistrationError::Invalid("invalid host route pattern"));
        }
        Ok(Self { method, route: route.to_owned() })
    }
    pub const fn method(&self) -> RequestMethod {
        self.method
    }
    pub fn route(&self) -> &str {
        &self.route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    Forbidden,
    TooManyRequests,
    Unavailable,
}

/// Plugins can annotate responses, but cannot change authentication, caching or CSP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseAnnotation {
    name: String,
    value: String,
}
impl ResponseAnnotation {
    pub fn new(name: &str, value: &str) -> Result<Self, HookRegistrationError> {
        if !name.starts_with("x-rbit-plugin-")
            || name.len() <= 14
            || name.len() > 64
            || !name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || value.len() > 256
            || value.bytes().any(|byte| !(32..=126).contains(&byte))
        {
            return Err(HookRegistrationError::Invalid("invalid plugin response annotation"));
        }
        Ok(Self { name: name.to_owned(), value: value.to_owned() })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    Continue(Vec<ResponseAnnotation>),
    Reject(Rejection),
}

pub type HookFuture<'a> = Pin<Box<dyn Future<Output = HookDecision> + Send + 'a>>;

/// Trusted native callbacks must stay cooperative and use their invocation future.
/// They cannot spawn detached work or perform blocking work on the request executor.
pub trait RequestHook: Debug + Send + Sync {
    fn call<'a>(&'a self, request: &'a RequestMetadata) -> HookFuture<'a>;
}

#[derive(Debug, Clone)]
pub struct HookRegistration {
    id: String,
    order: i16,
    deadline: Duration,
    callback: Arc<dyn RequestHook>,
}
impl HookRegistration {
    pub fn new(id: &str, order: i16, deadline: Duration, callback: Arc<dyn RequestHook>) -> Result<Self, HookRegistrationError> {
        if id.is_empty()
            || id.len() > 64
            || !id.starts_with(|character: char| character.is_ascii_lowercase())
            || !id.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-'))
            || !(-1024..=1024).contains(&order)
            || deadline < Duration::from_millis(1)
            || deadline > MAX_HOOK_DEADLINE
        {
            return Err(HookRegistrationError::Invalid("invalid hook descriptor"));
        }
        Ok(Self { id: id.to_owned(), order, deadline, callback })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub const fn order(&self) -> i16 {
        self.order
    }
    pub const fn deadline(&self) -> Duration {
        self.deadline
    }
    pub(crate) fn callback(&self) -> &dyn RequestHook {
        self.callback.as_ref()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HookRegistrationError {
    #[error("{0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Registry(#[from] rbit_plugin_runtime::ExtensionRegistryError),
}
