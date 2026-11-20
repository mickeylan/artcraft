use serde_json::Value;
use url::Url;

use crate::client::LocalPlatformClient;
use crate::error::local_platform_error::{body_excerpt, map_send_error, LocalPlatformError};

/// The capability endpoint named by the integration requirements (`GET
/// /api/model-catalog`). It is unauthenticated by design for the first
/// increment: the platform's deployment authentication is still unconfirmed.
pub const MODEL_CATALOG_PATH: &str = "api/model-catalog";

pub const ACCEPT_JSON: &str = "application/json";

/// The platform's capability document exactly as returned.
///
/// The requirements document lists the concepts the catalog must expose
/// (template code, mode, display name, enabled flag, parameters, file slots),
/// but not their JSON names. Extraction is therefore deferred until the
/// platform team delivers a sanitized sample; guessing field names would
/// produce a contract test that only proves our own guess.
#[derive(Debug, Clone)]
pub struct ModelCatalogDocument {
  pub source_url: Url,
  pub http_status: u16,
  pub json: Value,
}

/// The shape of the document's top level, for a settings-pane summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogTopLevelKind {
  Object,
  Array,
  Scalar,
}

/// What the connection probe can safely say without knowing field names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogSummary {
  pub top_level_kind: CatalogTopLevelKind,
  /// Set only for a top-level array.
  pub entry_count: Option<usize>,
  /// Set only for a top-level object.
  pub top_level_key_count: usize,
}

impl ModelCatalogDocument {
  pub fn summary(&self) -> CatalogSummary {
    match &self.json {
      Value::Object(entries) => CatalogSummary { top_level_kind: CatalogTopLevelKind::Object, entry_count: None, top_level_key_count: entries.len() },
      Value::Array(entries) => CatalogSummary { top_level_kind: CatalogTopLevelKind::Array, entry_count: Some(entries.len()), top_level_key_count: 0 },
      _ => CatalogSummary { top_level_kind: CatalogTopLevelKind::Scalar, entry_count: None, top_level_key_count: 0 },
    }
  }
}

/// Query the platform's capability document.
///
/// This doubles as the connection check: a success proves the configured
/// address answers the documented route with JSON. No other endpoint is probed
/// because none is documented for health checks.
pub async fn get_model_catalog(client: &LocalPlatformClient) -> Result<ModelCatalogDocument, LocalPlatformError> {
  let url = client.model_catalog_url().clone();
  let endpoint = url.path().to_string();

  let response = client.http().get(url.clone()).header(reqwest::header::ACCEPT, ACCEPT_JSON).send().await.map_err(|err| map_send_error(&endpoint, client.request_timeout(), err))?;

  let status = response.status();
  let body = response.text().await.map_err(|err| map_send_error(&endpoint, client.request_timeout(), err))?;

  if !status.is_success() {
    return Err(LocalPlatformError::UnexpectedStatus { endpoint, status_code: status.as_u16(), body_excerpt: body_excerpt(&body) });
  }

  let json: Value = serde_json::from_str(&body).map_err(|source| LocalPlatformError::InvalidJson { endpoint, body_excerpt: body_excerpt(&body), source })?;

  Ok(ModelCatalogDocument { source_url: url, http_status: status.as_u16(), json })
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::{BufRead, BufReader, Write};
  use std::net::TcpListener;
  use std::sync::mpsc::{self, Receiver};
  use std::time::Duration;

  const STUB_TIMEOUT: Duration = Duration::from_secs(5);
  const CATALOG_JSON_OBJECT: &str = r#"{"templates":[{"code":"h3_i2v","mode":"image_to_video"}]}"#;
  const CATALOG_JSON_ARRAY: &str = r#"[{"code":"h3_i2v"},{"code":"h3_i2v_turbo"}]"#;

  struct StubResponse {
    status_line: &'static str,
    content_type: &'static str,
    body: String,
  }

  struct CapturedRequest {
    request_line: String,
    headers: Vec<(String, String)>,
  }

  impl CapturedRequest {
    fn header(&self, name: &str) -> Option<&str> {
      self.headers.iter().find(|(header_name, _)| header_name.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }
  }

  fn ok_json(body: &str) -> StubResponse {
    StubResponse { status_line: "200 OK", content_type: ACCEPT_JSON, body: body.to_string() }
  }

  /// Bind a one-shot HTTP server on a loopback port and return its base URL.
  ///
  /// This is a contract test, not a mock of our own code: the request under
  /// test travels over a real socket and the captured bytes are asserted.
  fn spawn_stub_server(response: StubResponse) -> (Url, Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("binds the stub server");
    let address = listener.local_addr().expect("reads the stub address");
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
      let (mut stream, _) = listener.accept().expect("accepts one request");
      let mut reader = BufReader::new(stream.try_clone().expect("clones the stream"));

      let mut request_line = String::new();
      reader.read_line(&mut request_line).expect("reads the request line");

      let mut headers = Vec::new();
      loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("reads a header line");
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
          break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
          headers.push((name.trim().to_string(), value.trim().to_string()));
        }
      }

      let _ = sender.send(CapturedRequest { request_line: request_line.trim_end_matches(['\r', '\n']).to_string(), headers });

      let response = format!("HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.status_line, response.content_type, response.body.len(), response.body,);
      stream.write_all(response.as_bytes()).expect("writes the stub response");
      stream.flush().expect("flushes the stub response");
    });

    let base_url = Url::parse(&format!("http://{address}")).expect("builds the stub URL");
    (base_url, receiver)
  }

  mod request_contract {
    use super::*;

    #[tokio::test]
    async fn sends_a_get_to_the_documented_path_without_credentials() {
      let (base_url, captured) = spawn_stub_server(ok_json(CATALOG_JSON_OBJECT));
      let client = LocalPlatformClient::new(base_url).expect("builds the client");

      let document = get_model_catalog(&client).await.expect("fetch succeeds");

      assert_eq!(document.http_status, 200);
      assert_eq!(document.summary().top_level_kind, CatalogTopLevelKind::Object);

      let request = captured.recv_timeout(STUB_TIMEOUT).expect("captures the request");
      assert_eq!(request.request_line, "GET /api/model-catalog HTTP/1.1");
      assert_eq!(request.header("accept"), Some(ACCEPT_JSON));
      assert_eq!(request.header("authorization"), None, "the local channel must not use an ArtCraft token");
      assert_eq!(request.header("cookie"), None, "the local channel must not send webview cookies");
    }

    #[tokio::test]
    async fn keeps_a_base_path_prefix() {
      let (base_url, captured) = spawn_stub_server(ok_json(CATALOG_JSON_OBJECT));
      let prefixed = base_url.join("platform").expect("builds a prefixed URL");
      let client = LocalPlatformClient::new(prefixed).expect("builds the client");

      get_model_catalog(&client).await.expect("fetch succeeds");

      let request = captured.recv_timeout(STUB_TIMEOUT).expect("captures the request");
      assert_eq!(request.request_line, "GET /platform/api/model-catalog HTTP/1.1");
    }
  }

  mod response_contract {
    use super::*;

    #[tokio::test]
    async fn reports_a_top_level_array_and_its_entry_count() {
      let (base_url, _captured) = spawn_stub_server(ok_json(CATALOG_JSON_ARRAY));
      let client = LocalPlatformClient::new(base_url).expect("builds the client");

      let document = get_model_catalog(&client).await.expect("fetch succeeds");
      let summary = document.summary();

      assert_eq!(summary.top_level_kind, CatalogTopLevelKind::Array);
      assert_eq!(summary.entry_count, Some(2));
      assert_eq!(summary.top_level_key_count, 0);
    }

    #[tokio::test]
    async fn counts_top_level_object_keys_without_naming_them() {
      let (base_url, _captured) = spawn_stub_server(ok_json(CATALOG_JSON_OBJECT));
      let client = LocalPlatformClient::new(base_url).expect("builds the client");

      let document = get_model_catalog(&client).await.expect("fetch succeeds");
      let summary = document.summary();

      assert_eq!(summary.top_level_kind, CatalogTopLevelKind::Object);
      assert_eq!(summary.top_level_key_count, 1);
      assert_eq!(summary.entry_count, None);
    }

    #[tokio::test]
    async fn a_non_success_status_is_an_error_with_the_body() {
      let (base_url, _captured) = spawn_stub_server(StubResponse { status_line: "503 Service Unavailable", content_type: "text/plain", body: "platform is starting".to_string() });
      let client = LocalPlatformClient::new(base_url).expect("builds the client");

      match get_model_catalog(&client).await {
        Err(LocalPlatformError::UnexpectedStatus { status_code, body_excerpt, endpoint }) => {
          assert_eq!(status_code, 503);
          assert_eq!(endpoint, "/api/model-catalog");
          assert!(body_excerpt.contains("platform is starting"));
        },
        other => panic!("expected UnexpectedStatus, got {other:?}"),
      }
    }

    #[tokio::test]
    async fn a_non_json_success_body_is_an_error_with_the_body() {
      let (base_url, _captured) = spawn_stub_server(StubResponse { status_line: "200 OK", content_type: "text/html", body: "<html>not the API</html>".to_string() });
      let client = LocalPlatformClient::new(base_url).expect("builds the client");

      match get_model_catalog(&client).await {
        Err(LocalPlatformError::InvalidJson { body_excerpt, endpoint, .. }) => {
          assert_eq!(endpoint, "/api/model-catalog");
          assert!(body_excerpt.contains("not the API"));
        },
        other => panic!("expected InvalidJson, got {other:?}"),
      }
    }

    #[tokio::test]
    async fn an_empty_body_is_an_error_not_an_empty_catalog() {
      let (base_url, _captured) = spawn_stub_server(ok_json(""));
      let client = LocalPlatformClient::new(base_url).expect("builds the client");

      assert!(matches!(get_model_catalog(&client).await, Err(LocalPlatformError::InvalidJson { .. })));
    }
  }
}
