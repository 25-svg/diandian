use super::model::{valid_text, valid_token, LicenseError, MAX_LEASE_BYTES};
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    redirect::Policy,
    StatusCode, Url,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 65_536;
const TIMEOUT: Duration = Duration::from_secs(15);

// These secret-bearing structs deliberately do not derive Debug.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationResponse {
    pub device_token: String,
    pub lease: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenewalResponse {
    pub lease: String,
}

pub struct DistributionClient {
    endpoint: Url,
    http: reqwest::Client,
}
impl DistributionClient {
    pub fn compiled() -> Result<Self, LicenseError> {
        Self::new(option_env!("DIANDIAN_UPDATE_ENDPOINT").ok_or(LicenseError::Configuration)?)
    }
    pub fn new(endpoint: &str) -> Result<Self, LicenseError> {
        Self::build(endpoint, TIMEOUT, false)
    }
    fn build(
        endpoint: &str,
        timeout: Duration,
        allow_loopback: bool,
    ) -> Result<Self, LicenseError> {
        if endpoint.len() > 2048 {
            return Err(LicenseError::Configuration);
        }
        let endpoint = Url::parse(endpoint).map_err(|_| LicenseError::Configuration)?;
        let loopback = cfg!(test)
            && allow_loopback
            && endpoint.scheme() == "http"
            && endpoint.host_str() == Some("127.0.0.1");
        if (endpoint.scheme() != "https" && !loopback)
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || endpoint.path() != "/"
        {
            return Err(LicenseError::Configuration);
        }
        // Device credentials must not be silently routed through ambient proxies.
        // Redirects are forbidden even on the same host; compressed body size is
        // measured after decompression by the streaming limit below.
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            .redirect(Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| LicenseError::Configuration)?;
        Ok(Self { endpoint, http })
    }
    async fn post<T: Serialize>(
        &self,
        path: &str,
        token: Option<&str>,
        body: Option<&T>,
        expected: StatusCode,
    ) -> Result<Vec<u8>, LicenseError> {
        let url = self
            .endpoint
            .join(path)
            .map_err(|_| LicenseError::Configuration)?;
        let mut request = self.http.post(url);
        if let Some(token) = token {
            if !valid_token(token) {
                return Err(LicenseError::InvalidInput);
            }
            let mut header = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| LicenseError::InvalidInput)?;
            header.set_sensitive(true);
            request = request.header(AUTHORIZATION, header);
        }
        if let Some(body) = body {
            let bytes = serde_json::to_vec(body).map_err(|_| LicenseError::InvalidInput)?;
            if bytes.len() > 4096 {
                return Err(LicenseError::InvalidInput);
            }
            request = request
                .header("content-type", "application/json")
                .body(bytes);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| LicenseError::Unavailable)?;
        let status = response.status();
        if status.is_redirection() {
            return Err(LicenseError::Unavailable);
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
        {
            return Err(LicenseError::InvalidResponse);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| LicenseError::Unavailable)?
        {
            if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(bytes.len()) {
                return Err(LicenseError::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        if status != expected {
            return Err(service_error(status, &bytes));
        }
        Ok(bytes)
    }
    /// Response is transport data: verify the signed lease with the compiled
    /// public key before trusting its server time, persisting, or authorizing.
    pub async fn activate(
        &self,
        code: &str,
        install_id: &str,
        label: &str,
    ) -> Result<ActivationResponse, LicenseError> {
        if !valid_text(code, 256) || !valid_text(install_id, 256) || !valid_text(label, 256) {
            return Err(LicenseError::InvalidInput);
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Input<'a> {
            code: &'a str,
            install_id: &'a str,
            label: &'a str,
        }
        let bytes = self
            .post(
                "/v1/activate",
                None,
                Some(&Input {
                    code,
                    install_id,
                    label,
                }),
                StatusCode::CREATED,
            )
            .await?;
        let response: ActivationResponse = decode(&bytes)?;
        if !valid_token(&response.device_token) || !valid_text(&response.lease, MAX_LEASE_BYTES) {
            return Err(LicenseError::InvalidResponse);
        }
        Ok(response)
    }
    pub async fn renew(&self, device_token: &str) -> Result<RenewalResponse, LicenseError> {
        let bytes = self
            .post::<()>("/v1/lease/renew", Some(device_token), None, StatusCode::OK)
            .await?;
        let response: RenewalResponse = decode(&bytes)?;
        if !valid_text(&response.lease, MAX_LEASE_BYTES) {
            return Err(LicenseError::InvalidResponse);
        }
        Ok(response)
    }
    pub async fn report_event(
        &self,
        device_token: &str,
        release_id: Option<&str>,
        current_version: &str,
        event_type: &str,
    ) -> Result<(), LicenseError> {
        if !valid_text(current_version, 128)
            || ![
                "check",
                "download_started",
                "download_failed",
                "install_succeeded",
                "install_failed",
            ]
            .contains(&event_type)
            || (event_type == "check") != release_id.is_none()
            || release_id.is_some_and(|id| {
                id.is_empty()
                    || id.len() > 256
                    || !id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
        {
            return Err(LicenseError::InvalidInput);
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Input<'a> {
            release_id: Option<&'a str>,
            current_version: &'a str,
            event_type: &'a str,
        }
        let bytes = self
            .post(
                "/v1/update-events",
                Some(device_token),
                Some(&Input {
                    release_id,
                    current_version,
                    event_type,
                }),
                StatusCode::NO_CONTENT,
            )
            .await?;
        if !bytes.is_empty() {
            return Err(LicenseError::InvalidResponse);
        }
        Ok(())
    }

    pub async fn report_event_idempotent(
        &self,
        device_token: &str,
        release_id: Option<&str>,
        current_version: &str,
        event_type: &str,
        client_event_id: &str,
    ) -> Result<(), LicenseError> {
        if event_type != "install_succeeded"
            || uuid::Uuid::parse_str(client_event_id).is_err()
            || !valid_text(current_version, 128)
            || release_id.is_none_or(|id| {
                id.is_empty()
                    || id.len() > 256
                    || !id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            })
        {
            return Err(LicenseError::InvalidInput);
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Input<'a> {
            release_id: Option<&'a str>,
            current_version: &'a str,
            event_type: &'a str,
            client_event_id: &'a str,
        }
        let bytes = self
            .post(
                "/v1/update-events",
                Some(device_token),
                Some(&Input {
                    release_id,
                    current_version,
                    event_type,
                    client_event_id,
                }),
                StatusCode::NO_CONTENT,
            )
            .await?;
        if !bytes.is_empty() {
            return Err(LicenseError::InvalidResponse);
        }
        Ok(())
    }
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, LicenseError> {
    serde_json::from_slice(bytes).map_err(|_| LicenseError::InvalidResponse)
}
fn service_error(status: StatusCode, bytes: &[u8]) -> LicenseError {
    #[derive(Deserialize)]
    struct Envelope {
        error: ErrorCode,
    }
    #[derive(Deserialize)]
    struct ErrorCode {
        code: String,
    }
    let Ok(envelope) = serde_json::from_slice::<Envelope>(bytes) else {
        return LicenseError::Unavailable;
    };
    match (status.as_u16(), envelope.error.code.as_str()) {
        (403, "DEVICE_REVOKED") => LicenseError::Revoked,
        (401, "DEVICE_TOKEN_INVALID") => LicenseError::InvalidToken,
        (400, "ACTIVATION_CODE_INVALID") => LicenseError::InvalidCode,
        (409, "ACTIVATION_CODE_USED") => LicenseError::CodeUsed,
        (409, "INSTALLATION_ALREADY_ACTIVATED") => LicenseError::AlreadyActivated,
        _ => LicenseError::Unavailable,
    }
}

#[cfg(test)]
mod private_distribution_client_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    fn fixture(
        status: &str,
        body: String,
        extra: &str,
        delay: Duration,
    ) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let status = status.to_owned();
        let extra = extra.to_owned();
        let handle = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 1024];
            loop {
                let n = socket.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let size = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= end + 4 + size {
                        break;
                    }
                }
            }
            thread::sleep(delay);
            let response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n{extra}Connection: close\r\n\r\n{body}",body.len());
            let _ = socket.write_all(response.as_bytes());
            String::from_utf8(request).unwrap()
        });
        (endpoint, handle)
    }
    fn client(endpoint: &str, timeout: Duration) -> DistributionClient {
        DistributionClient::build(endpoint, timeout, true).unwrap()
    }
    fn token() -> String {
        "x".repeat(43)
    }
    #[tokio::test]
    async fn activation_and_renewal_match_worker_contract_and_header_only_token() {
        let (endpoint, server) = fixture(
            "201 Created",
            format!(r#"{{"deviceToken":"{}","lease":"signed.lease"}}"#, token()),
            "",
            Duration::ZERO,
        );
        let c = client(&endpoint, Duration::from_secs(2));
        assert_eq!(
            c.activate("code", "install", "label")
                .await
                .unwrap()
                .device_token,
            token()
        );
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1/activate HTTP/1.1"));
        assert!(!request.to_ascii_lowercase().contains("authorization"));
        let body: serde_json::Value =
            serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"code":"code","installId":"install","label":"label"})
        );
        let (endpoint, server) = fixture(
            "200 OK",
            r#"{"lease":"signed.lease"}"#.into(),
            "",
            Duration::ZERO,
        );
        client(&endpoint, Duration::from_secs(2))
            .renew(&token())
            .await
            .unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1/lease/renew HTTP/1.1"));
        assert!(request
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {}", token())));
        assert_eq!(request.matches(&token()).count(), 1);
    }
    #[tokio::test]
    async fn event_uses_header_and_camel_case_body() {
        let (endpoint, server) = fixture("204 No Content", String::new(), "", Duration::ZERO);
        client(&endpoint, Duration::from_secs(2))
            .report_event(&token(), None, "2.21.1", "check")
            .await
            .unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1/update-events HTTP/1.1"));
        assert_eq!(request.matches(&token()).count(), 1);
        let body: serde_json::Value =
            serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"releaseId":null,"currentVersion":"2.21.1","eventType":"check"})
        );
    }
    #[tokio::test]
    async fn install_receipt_event_carries_only_stable_id_and_safe_payload() {
        let (endpoint, server) = fixture("204 No Content", String::new(), "", Duration::ZERO);
        let event_id = "123e4567-e89b-42d3-a456-426614174000";
        client(&endpoint, Duration::from_secs(2))
            .report_event_idempotent(
                &token(),
                Some("release_1"),
                "2.22.0",
                "install_succeeded",
                event_id,
            )
            .await
            .unwrap();
        let request = server.join().unwrap();
        let body: serde_json::Value =
            serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "releaseId":"release_1", "currentVersion":"2.22.0",
                "eventType":"install_succeeded", "clientEventId":event_id
            })
        );
        assert!(!body.to_string().contains(&token()));
    }
    #[tokio::test]
    async fn errors_are_fixed_and_revocation_is_distinct() {
        for (status, body, expected) in [
            (
                "403 Forbidden",
                r#"{"error":{"code":"DEVICE_REVOKED","message":"secret"}}"#.to_string(),
                LicenseError::Revoked,
            ),
            (
                "500 Internal Server Error",
                token(),
                LicenseError::Unavailable,
            ),
        ] {
            let (endpoint, server) = fixture(status, body, "", Duration::ZERO);
            let error = client(&endpoint, Duration::from_secs(2))
                .renew(&token())
                .await
                .err()
                .unwrap();
            assert_eq!(error, expected);
            assert!(!format!("{error:?} {error}").contains(&token()));
            server.join().unwrap();
        }
    }
    #[tokio::test]
    async fn redirects_timeouts_and_large_responses_fail_closed() {
        for (status, body, extra, delay, expected) in [
            (
                "302 Found",
                String::new(),
                "Location: http://127.0.0.1:1/leak\r\n",
                Duration::ZERO,
                LicenseError::Unavailable,
            ),
            (
                "500 Internal Server Error",
                "a".repeat(65537),
                "",
                Duration::ZERO,
                LicenseError::InvalidResponse,
            ),
            (
                "200 OK",
                String::new(),
                "",
                Duration::from_millis(200),
                LicenseError::Unavailable,
            ),
        ] {
            let (endpoint, server) = fixture(status, body, extra, delay);
            assert_eq!(
                client(&endpoint, Duration::from_millis(80))
                    .renew(&token())
                    .await
                    .err()
                    .unwrap(),
                expected
            );
            server.join().unwrap();
        }
    }
    #[test]
    fn production_endpoint_rejects_insecure_or_credential_bearing_urls() {
        for endpoint in [
            "http://example.com",
            "https://user:secret@example.com",
            "https://example.com/?token=x",
            "https://example.com/#x",
            "https://example.com/base",
        ] {
            assert!(DistributionClient::new(endpoint).is_err());
        }
        assert!(DistributionClient::new("https://example.com").is_ok());
    }

    #[tokio::test]
    async fn streaming_cap_does_not_depend_on_content_length() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            socket.read(&mut request).unwrap();
            let _ = socket
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\n\r\n");
            let _ = socket.write_all(&vec![b'a'; 65537]);
        });
        assert_eq!(
            client(&endpoint, Duration::from_secs(2))
                .renew(&token())
                .await
                .err()
                .unwrap(),
            LicenseError::InvalidResponse
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn missing_duplicate_and_unknown_response_fields_are_not_accepted() {
        for body in [
            r#"{}"#,
            r#"{"lease":"one","lease":"two"}"#,
            r#"{"lease":"one","extra":"unexpected"}"#,
        ] {
            let (endpoint, server) = fixture("200 OK", body.into(), "", Duration::ZERO);
            assert_eq!(
                client(&endpoint, Duration::from_secs(2))
                    .renew(&token())
                    .await
                    .err()
                    .unwrap(),
                LicenseError::InvalidResponse
            );
            server.join().unwrap();
        }
        let c = DistributionClient::new("https://example.com").unwrap();
        assert_eq!(
            c.renew("bad\r\nheader").await.err().unwrap(),
            LicenseError::InvalidInput
        );
        assert_eq!(
            c.activate(&"x".repeat(257), "install", "label")
                .await
                .err()
                .unwrap(),
            LicenseError::InvalidInput
        );
        assert_eq!(
            c.report_event(&token(), Some("release"), "2.21.1", "check")
                .await
                .err()
                .unwrap(),
            LicenseError::InvalidInput
        );
    }

    #[tokio::test]
    async fn redirect_destination_never_receives_a_connection() {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let location = format!(
            "Location: http://{}/leak\r\n",
            destination.local_addr().unwrap()
        );
        let (endpoint, server) = fixture(
            "307 Temporary Redirect",
            String::new(),
            &location,
            Duration::ZERO,
        );
        assert_eq!(
            client(&endpoint, Duration::from_millis(300))
                .renew(&token())
                .await
                .err()
                .unwrap(),
            LicenseError::Unavailable
        );
        server.join().unwrap();
        assert_eq!(
            destination.accept().err().unwrap().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
