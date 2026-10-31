//! Bounded multipart transport for the sealed, administrator-only upload capability.

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, FromRequest, Multipart, Request},
    http::{HeaderMap, StatusCode, header},
};
use rbit_services::plugin_pages::{MAX_UPLOAD_BYTES, UPLOAD_FIELD, UploadData};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const FRAMING_BYTES: usize = 8 * 1024;
const PROOF_BYTES: usize = 128;

pub(super) fn validate_headers(headers: &HeaderMap, max_bytes: usize) -> Result<(), StatusCode> {
    if max_bytes == 0 || max_bytes > MAX_UPLOAD_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    if headers.contains_key(header::CONTENT_ENCODING) || headers.get_all(header::CONTENT_TYPE).iter().count() != 1 {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() <= 256)
        .ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    let mut parts = content_type.split(';').map(str::trim);
    if !parts.next().is_some_and(|value| value.eq_ignore_ascii_case("multipart/form-data")) {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let (name, boundary) = parts.next().and_then(|part| part.split_once('=')).ok_or(StatusCode::BAD_REQUEST)?;
    if !name.trim().eq_ignore_ascii_case("boundary") || parts.next().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let boundary = boundary.trim();
    let boundary = if boundary.starts_with('"') {
        boundary.strip_prefix('"').and_then(|value| value.strip_suffix('"')).ok_or(StatusCode::BAD_REQUEST)?
    } else {
        boundary
    };
    if boundary.is_empty()
        || boundary.len() > 70
        || !boundary.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"'()+_,-./:=?".contains(&byte))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if headers.get_all(header::CONTENT_LENGTH).iter().count() > 1 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(length) = headers.get(header::CONTENT_LENGTH) {
        let length = length.to_str().map_err(|_| StatusCode::BAD_REQUEST)?;
        if length.is_empty() || !length.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let length = length.parse::<usize>().map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
        if length > max_bytes + FRAMING_BYTES {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
    }
    Ok(())
}

pub(super) async fn parse(
    body: Body,
    headers: &HeaderMap,
    cancellation: &CancellationToken,
    max_bytes: usize,
) -> Result<(String, UploadData), StatusCode> {
    parse_with_timeout(body, headers, cancellation, max_bytes, Duration::from_secs(10)).await
}

async fn parse_with_timeout(
    mut body: Body,
    headers: &HeaderMap,
    cancellation: &CancellationToken,
    max_bytes: usize,
    timeout: Duration,
) -> Result<(String, UploadData), StatusCode> {
    use http_body::Body as _;
    validate_headers(headers, max_bytes)?;
    let total = max_bytes + FRAMING_BYTES;
    let operation = async {
        // Count the complete wire body, including MIME epilogues that a multipart
        // decoder may legitimately ignore after its final boundary.
        let mut bytes = Vec::with_capacity(total);
        while let Some(frame) = std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await {
            let data = frame.map_err(|_| StatusCode::BAD_REQUEST)?.into_data().map_err(|_| StatusCode::BAD_REQUEST)?;
            if data.len() > total.saturating_sub(bytes.len()) {
                return Err(StatusCode::PAYLOAD_TOO_LARGE);
            }
            bytes.extend_from_slice(&data);
        }
        drop(body);
        let mut request = Request::new(Body::from(bytes));
        *request.headers_mut() = headers.clone();
        DefaultBodyLimit::max(total).apply(&mut request);
        let mut multipart = Multipart::from_request(request, &()).await.map_err(|_| StatusCode::BAD_REQUEST)?;
        let proof = multipart.next_field().await.map_err(|error| error.status())?.ok_or(StatusCode::BAD_REQUEST)?;
        if proof.name() != Some("_csrf") || proof.file_name().is_some() {
            return Err(StatusCode::BAD_REQUEST);
        }
        let proof = field(proof, PROOF_BYTES).await?;
        let proof = String::from_utf8(proof).map_err(|_| StatusCode::BAD_REQUEST)?;
        if proof.is_empty() {
            return Err(StatusCode::FORBIDDEN);
        }
        let upload = multipart.next_field().await.map_err(|error| error.status())?.ok_or(StatusCode::BAD_REQUEST)?;
        if upload.name() != Some(UPLOAD_FIELD) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let upload = field(upload, max_bytes).await?;
        if multipart.next_field().await.map_err(|error| error.status())?.is_some() {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok((proof, UploadData::new(upload).map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?))
    };
    tokio::select! { biased;
        () = cancellation.cancelled() => Err(StatusCode::SERVICE_UNAVAILABLE),
        result = tokio::time::timeout(timeout, operation) => result.unwrap_or(Err(StatusCode::REQUEST_TIMEOUT)),
    }
}

async fn field(mut field: axum::extract::multipart::Field<'_>, max_bytes: usize) -> Result<Vec<u8>, StatusCode> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|error| error.status())? {
        if chunk.len() > max_bytes.saturating_sub(bytes.len()) {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "multipart/form-data; boundary=rbit".parse().unwrap());
        headers
    }
    fn wire(fields: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (name, value) in fields {
            bytes.extend_from_slice(format!("--rbit\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes());
            bytes.extend_from_slice(value);
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(b"--rbit--\r\n");
        bytes
    }

    #[tokio::test]
    async fn multipart_preserves_binary_and_accepts_the_exact_archive_limit_with_framing() {
        let archive = vec![0xff; MAX_UPLOAD_BYTES];
        let bytes = wire(&[("_csrf", b"proof"), ("pack", &archive)]);
        let (proof, parsed) = parse(Body::from(bytes), &headers(), &CancellationToken::new(), MAX_UPLOAD_BYTES).await.unwrap();
        assert_eq!(proof, "proof");
        assert_eq!(parsed.bytes(), archive);
    }

    #[tokio::test]
    async fn multipart_rejects_missing_reordered_duplicate_unknown_and_oversize_fields() {
        for fields in [
            vec![("pack", b"bytes".as_slice())],
            vec![("pack", b"bytes".as_slice()), ("_csrf", b"proof")],
            vec![("_csrf", b"proof".as_slice()), ("other", b"bytes")],
            vec![("_csrf", b"proof".as_slice()), ("pack", b"bytes"), ("pack", b"more")],
            vec![("_csrf", b"proof".as_slice()), ("pack", b"bytes"), ("_csrf", b"again")],
        ] {
            assert_eq!(
                parse(Body::from(wire(&fields)), &headers(), &CancellationToken::new(), 64).await.unwrap_err(),
                StatusCode::BAD_REQUEST
            );
        }
        for (proof, pack) in [(vec![b'x'; PROOF_BYTES + 1], vec![0]), (b"proof".to_vec(), vec![0; 65])] {
            assert_eq!(
                parse(Body::from(wire(&[("_csrf", &proof), ("pack", &pack)])), &headers(), &CancellationToken::new(), 64)
                    .await
                    .unwrap_err(),
                StatusCode::PAYLOAD_TOO_LARGE
            );
        }
        let mut truncated = wire(&[("_csrf", b"proof"), ("pack", b"bytes")]);
        truncated.truncate(truncated.len() - 10);
        assert_eq!(parse(Body::from(truncated), &headers(), &CancellationToken::new(), 64).await.unwrap_err(), StatusCode::BAD_REQUEST);
        let mut epilogue = wire(&[("_csrf", b"proof"), ("pack", b"bytes")]);
        epilogue.extend(vec![b'x'; FRAMING_BYTES]);
        assert_eq!(
            parse(Body::from(epilogue), &headers(), &CancellationToken::new(), 64).await.unwrap_err(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[test]
    fn multipart_header_validation_is_cheap_strict_and_never_compressed() {
        for content_type in [
            "text/plain",
            "application/x-www-form-urlencoded",
            "multipart/form-data",
            "multipart/form-data; boundary=",
            "multipart/form-data; boundary=a; boundary=b",
        ] {
            let mut headers = headers();
            headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
            assert!(validate_headers(&headers, 64).is_err(), "{content_type}");
        }
        let mut compressed = headers();
        compressed.insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
        assert_eq!(validate_headers(&compressed, 64), Err(StatusCode::UNSUPPORTED_MEDIA_TYPE));
        let mut too_large = headers();
        too_large.insert(header::CONTENT_LENGTH, (64 + FRAMING_BYTES + 1).to_string().parse().unwrap());
        assert_eq!(validate_headers(&too_large, 64), Err(StatusCode::PAYLOAD_TOO_LARGE));
    }

    #[tokio::test]
    async fn multipart_pending_upload_is_cancelled_or_hits_an_absolute_deadline() {
        for cancel in [false, true] {
            let cancellation = CancellationToken::new();
            if cancel {
                cancellation.cancel();
            }
            let body = Body::from_stream(futures_util::stream::pending::<Result<axum::body::Bytes, std::io::Error>>());
            let result = parse_with_timeout(body, &headers(), &cancellation, 64, Duration::from_millis(20)).await.unwrap_err();
            assert_eq!(result, if cancel { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::REQUEST_TIMEOUT });
        }
    }
}
