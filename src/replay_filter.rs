use crate::access_log::ReplayRequest;
use crate::{ReplayFilter, RunError};

const MAX_URI_PATTERNS: usize = 32;
const MAX_PATTERN_BYTES: usize = 256;

pub(crate) fn apply(
    requests: Vec<ReplayRequest>,
    filter: &ReplayFilter,
) -> Result<(Vec<ReplayRequest>, u64), RunError> {
    validate(filter)?;
    let original = requests.len();
    let requests: Vec<_> = requests
        .into_iter()
        .filter(|request| {
            (filter.allowed_methods.is_empty() || filter.allowed_methods.contains(&request.method))
                && (filter.allowed_uris.is_empty()
                    || filter
                        .allowed_uris
                        .iter()
                        .any(|pattern| glob_matches(pattern, &request.path)))
        })
        .collect();
    if requests.is_empty() {
        return Err(RunError::InvalidConfig(
            "replay whitelist excludes all requests".into(),
        ));
    }
    let filtered = (original - requests.len()) as u64;
    Ok((requests, filtered))
}

fn validate(filter: &ReplayFilter) -> Result<(), RunError> {
    if filter.allowed_uris.len() > MAX_URI_PATTERNS {
        return Err(RunError::InvalidConfig(
            "at most 32 allowed URI patterns may be supplied".into(),
        ));
    }
    for pattern in &filter.allowed_uris {
        if pattern.len() > MAX_PATTERN_BYTES
            || !pattern.starts_with('/')
            || !pattern.is_ascii()
            || pattern.bytes().any(|byte| byte <= b' ' || byte == b'#')
        {
            return Err(RunError::InvalidConfig(format!(
                "invalid allowed URI pattern: {pattern:?}"
            )));
        }
    }
    Ok(())
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut pattern_index, mut value_index) = (0, 0);
    let (mut star, mut retry_value) = (None, 0);
    while value_index < value.len() {
        if pattern.get(pattern_index) == Some(&value[value_index]) {
            pattern_index += 1;
            value_index += 1;
        } else if pattern.get(pattern_index) == Some(&b'*') {
            star = Some(pattern_index);
            pattern_index += 1;
            retry_value = value_index;
        } else if let Some(star_index) = star {
            retry_value += 1;
            value_index = retry_value;
            pattern_index = star_index + 1;
        } else {
            return false;
        }
    }
    while pattern.get(pattern_index) == Some(&b'*') {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Method;

    #[test]
    fn glob_matches_literal_and_multiple_wildcards() {
        assert!(glob_matches("/api/*/items/*", "/api/v1/items/42?full=1"));
        assert!(glob_matches("/health", "/health"));
        assert!(!glob_matches("/health", "/health/live"));
    }

    #[test]
    fn method_and_uri_filters_form_an_intersection() {
        let requests = vec![
            request(Method::Get, "/api/read"),
            request(Method::Post, "/api/write"),
            request(Method::Post, "/admin"),
        ];
        let filter = ReplayFilter {
            allowed_methods: vec![Method::Post],
            allowed_uris: vec!["/api/*".into()],
        };

        let (filtered, skipped) = apply(requests, &filter).unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, "/api/write");
        assert_eq!(skipped, 2);
    }

    #[test]
    fn rejects_filter_that_excludes_every_request() {
        let error = apply(
            vec![request(Method::Get, "/health")],
            &ReplayFilter {
                allowed_methods: vec![Method::Post],
                allowed_uris: Vec::new(),
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("excludes all requests"));
    }

    #[test]
    fn rejects_excessive_uri_pattern_work() {
        let too_many = ReplayFilter {
            allowed_methods: Vec::new(),
            allowed_uris: vec!["/*".into(); 33],
        };
        let too_long = ReplayFilter {
            allowed_methods: Vec::new(),
            allowed_uris: vec![format!("/{}", "a".repeat(256))],
        };

        assert!(validate(&too_many).is_err());
        assert!(validate(&too_long).is_err());
    }

    fn request(method: Method, path: &str) -> ReplayRequest {
        ReplayRequest::new(method, path.into(), Vec::new(), Vec::new(), false, None)
    }
}
