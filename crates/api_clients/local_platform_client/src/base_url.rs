use url::Url;

/// Parse a user-entered base URL and normalize it for endpoint joining.
///
/// Rejecting credentials here keeps secrets out of the settings file, error
/// messages and logs. Authentication, if the platform confirms it is needed,
/// belongs in a separate field with its own redaction rules.
pub(crate) fn parse_base_url(value: &str) -> Result<Url, String> {
  let trimmed = value.trim();
  if trimmed.is_empty() {
    return Err("the URL is empty".to_string());
  }
  let url = Url::parse(trimmed).map_err(|err| format!("not a valid URL: {err}"))?;
  normalize_base_url(url)
}

/// Validate and canonicalize a base URL.
///
/// The path always ends in `/` so that [`Url::join`] appends the endpoint
/// instead of replacing the last path segment.
pub(crate) fn normalize_base_url(mut url: Url) -> Result<Url, String> {
  match url.scheme() {
    "http" | "https" => {},
    other => return Err(format!("unsupported scheme '{other}'; use http or https")),
  }

  if url.host_str().is_none() {
    return Err("the URL has no host".to_string());
  }

  if !url.username().is_empty() || url.password().is_some() {
    return Err("credentials in the URL are not allowed".to_string());
  }

  if url.query().is_some() {
    return Err("the URL must not contain a query string".to_string());
  }

  if url.fragment().is_some() {
    return Err("the URL must not contain a fragment".to_string());
  }

  if !url.path().ends_with('/') {
    let path = format!("{}/", url.path());
    url.set_path(&path);
  }

  Ok(url)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn normalized(value: &str) -> Url {
    normalize_base_url(Url::parse(value).expect("valid test URL")).expect("normalizes")
  }

  fn rejection(value: &str) -> String {
    parse_base_url(value).expect_err("must be rejected")
  }

  mod parsing {
    use super::*;

    #[test]
    fn bare_host_and_port_get_a_trailing_slash() {
      assert_eq!(normalized("http://127.0.0.1:8000").as_str(), "http://127.0.0.1:8000/");
    }

    #[test]
    fn existing_trailing_slash_is_preserved() {
      assert_eq!(normalized("http://127.0.0.1:8000/").as_str(), "http://127.0.0.1:8000/");
    }

    #[test]
    fn path_prefix_is_preserved() {
      assert_eq!(normalized("http://127.0.0.1:8000/platform").as_str(), "http://127.0.0.1:8000/platform/");
    }

    #[test]
    fn normalization_is_idempotent() {
      let once = normalized("https://gpu-box.local:9000/platform");
      let twice = normalize_base_url(once.clone()).expect("normalizes twice");
      assert_eq!(once, twice);
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
      assert_eq!(parse_base_url("  http://127.0.0.1:8000  ").expect("parses").as_str(), "http://127.0.0.1:8000/");
    }

    #[test]
    fn empty_is_rejected() {
      assert!(rejection("   ").contains("empty"));
    }

    #[test]
    fn non_http_schemes_are_rejected() {
      assert!(rejection("ftp://127.0.0.1:8000").contains("unsupported scheme"));
    }

    #[test]
    fn a_bare_host_is_not_a_url() {
      assert!(rejection("localhost:8000").contains("unsupported scheme"));
    }

    #[test]
    fn credentials_are_rejected() {
      assert!(rejection("http://user:secret@127.0.0.1:8000").contains("credentials"));
    }

    #[test]
    fn rejected_credentials_are_not_echoed() {
      let message = rejection("http://user:secret@127.0.0.1:8000");
      assert!(!message.contains("secret"), "error must not echo the password: {message}");
      assert!(!message.contains("user"), "error must not echo the username: {message}");
    }

    #[test]
    fn query_strings_are_rejected() {
      assert!(rejection("http://127.0.0.1:8000/?token=secret").contains("query string"));
    }

    #[test]
    fn fragments_are_rejected() {
      assert!(rejection("http://127.0.0.1:8000/#section").contains("fragment"));
    }
  }
}
