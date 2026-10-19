//! Minimal S3 control requests for operations absent from object_store's public API.
use crate::config::{RoutedConnection, read_secret};
use aws_sigv4::{
    http_request::{
        PayloadChecksumKind, PercentEncodingMode, SignableBody, SignableRequest, SigningSettings,
        UriPathNormalizationMode, sign,
    },
    sign::v4,
};
use tonic::Status;
pub async fn request(
    conn: &RoutedConnection,
    method: &str,
    key: Option<&str>,
    query: &[(&str, &str)],
) -> Result<Vec<u8>, Status> {
    let (status, bytes) = signed(conn, method, key, query, &[], &[]).await?;
    if !status.is_success() {
        return Err(Status::failed_precondition(format!(
            "s3.control_request_failed:{}",
            status.as_u16()
        )));
    }
    Ok(bytes)
}
pub async fn signed(
    conn: &RoutedConnection,
    method: &str,
    key: Option<&str>,
    query: &[(&str, &str)],
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<(reqwest::StatusCode, Vec<u8>), Status> {
    let mut url = reqwest::Url::parse(&conn.endpoint)
        .map_err(|_| Status::invalid_argument("connection.endpoint_invalid"))?;
    if url.scheme() != "https" {
        return Err(Status::failed_precondition("connection.https_required"));
    }
    if conn.virtual_hosted {
        let host = format!(
            "{}.{}",
            conn.bucket,
            url.host_str()
                .ok_or_else(|| Status::invalid_argument("connection.endpoint_invalid"))?
        );
        url.set_host(Some(&host))
            .map_err(|_| Status::invalid_argument("connection.endpoint_invalid"))?;
    }
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| Status::invalid_argument("connection.endpoint_invalid"))?;
        path.pop_if_empty();
        if !conn.virtual_hosted {
            path.push(&conn.bucket);
        }
        if let Some(key) = key {
            for segment in key.split('/') {
                path.push(segment);
            }
        }
    }
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    let identity = aws_credential_types::Credentials::new(
        read_secret(&conn.access_key_file).map_err(internal)?,
        read_secret(&conn.secret_key_file).map_err(internal)?,
        None,
        None,
        "filedock-static",
    )
    .into();
    let mut settings = SigningSettings::default();
    settings.percent_encoding_mode = PercentEncodingMode::Single;
    settings.uri_path_normalization_mode = UriPathNormalizationMode::Disabled;
    settings.payload_checksum_kind = PayloadChecksumKind::XAmzSha256;
    let parameters = v4::SigningParams::builder()
        .identity(&identity)
        .region(&conn.region)
        .name("s3")
        .time(std::time::SystemTime::now())
        .settings(settings)
        .build()
        .map_err(internal)?
        .into();
    let signable = SignableRequest::new(
        method,
        url.as_str(),
        headers.iter().copied(),
        SignableBody::Bytes(body),
    )
    .map_err(internal)?;
    let (instructions, _) = sign(signable, &parameters).map_err(internal)?.into_parts();
    let mut signed = http::Request::builder()
        .method(method)
        .uri(url.as_str())
        .body(())
        .map_err(internal)?;
    for (name, value) in headers {
        signed.headers_mut().insert(
            http::header::HeaderName::from_bytes(name.as_bytes()).map_err(internal)?,
            http::HeaderValue::from_str(value).map_err(internal)?,
        );
    }
    instructions.apply_to_request_http1x(&mut signed);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(internal)?;
    let response = client
        .request(method.parse().map_err(internal)?, url)
        .headers(signed.headers().clone())
        .body(body.to_vec())
        .send()
        .await
        .map_err(|_| Status::unavailable("s3.control_request_failed"))?;
    let status = response.status();
    Ok((status, response.bytes().await.map_err(internal)?.to_vec()))
}
fn internal(e: impl std::fmt::Display) -> Status {
    Status::internal(e.to_string())
}
