//! The address of the client making a request.
//!
//! Milestone 1 workstream S owns this file (trusted reverse proxies, `X-Forwarded-For`).
//! Today it reports the TCP peer address.

use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::request::Parts,
};

/// The client's IP address, used for rate limiting and audit records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(pub IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Infallible;

    #[allow(clippy::unused_async_trait_impl)]
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|info| info.0.ip());
        Ok(Self(peer.unwrap_or(IpAddr::from([127, 0, 0, 1]))))
    }
}
