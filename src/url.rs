//! Centralised helpers for safely interpolating user-controlled values into
//! API URLs. Every command builds request paths by concatenating strings, so
//! both query parameters AND path segments (slugs, ids) must be encoded — a
//! `/` or `?` in a value would otherwise change the endpoint semantics.

/// Percent-encode a value for use inside a URL path segment.
/// (`/`, spaces, control chars, etc. become `%xx`.)
pub fn enc_path(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

/// Percent-encode a value for use inside a URL query parameter.
/// `urlencoding::encode` is identical to `enc_path`, but exposing two names
/// makes the call site self-documenting.
pub fn enc_query(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}
