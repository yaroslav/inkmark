//! URL-allowlist matching shared between the link and image filters.
//!
//! Two entry points, both using `url::Url::parse` as the single source
//! of truth for URL decomposition:
//!
//! - [`is_host_allowed`]: match the URL's host against a glob allowlist.
//! - [`is_scheme_allowed`]: match the URL's scheme against a string set.
//!
//! Both fail-open on URLs the parser can't resolve (relative paths,
//! anchors, protocol-relative, malformed input): such URLs have nothing
//! to check against, so they pass through unchanged.

use globset::GlobSet;

/// Return true when the URL should be kept by a host allowlist.
///
/// - **Has a host** (http/https URLs): lowercase and match against `set`.
/// - **No host** (relative `/foo`, anchor `#x`, `mailto:`, `tel:`,
///   `javascript:`, malformed input): out of scope for host allowlisting,
///   return true so the caller leaves it alone.
///
/// Fail-open on parse failure is deliberate: relative URLs must pass
/// through unchanged, and "can't parse" is how `url` signals that.
pub fn is_host_allowed(url: &str, set: &GlobSet) -> bool {
    match url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
    {
        Some(host) => set.is_match(&host),
        None => true,
    }
}

/// Return true when the URL's scheme is in the allowlist.
///
/// - **Parses as absolute URL**: check the scheme (lowercased by `url::Url`)
///   against `allowed`.
/// - **Doesn't parse** (relative `/foo`, anchor `#x`, protocol-relative
///   `//host/x`, malformed input): no scheme to check, return true.
///
/// `allowed` is a `&[String]` rather than a `HashSet` because every
/// realistic scheme allowlist has 2–5 entries, a linear scan of short
/// strings beats any hash table on CPU cache alone at that size. The
/// caller must pre-lowercase entries; `url::Url` normalizes schemes to
/// lowercase at parse time, so comparing against lowercase entries is
/// correct.
pub fn is_scheme_allowed(url: &str, allowed: &[String]) -> bool {
    match url::Url::parse(url) {
        Ok(parsed) => {
            let scheme = parsed.scheme();
            allowed.iter().any(|s| s == scheme)
        }
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_host_allowed, is_scheme_allowed};
    use globset::{Glob, GlobSetBuilder};

    fn host_set(patterns: &[&str]) -> globset::GlobSet {
        let mut b = GlobSetBuilder::new();
        for p in patterns {
            b.add(Glob::new(p).unwrap());
        }
        b.build().unwrap()
    }

    fn scheme_set(schemes: &[&str]) -> Vec<String> {
        schemes.iter().map(|s| s.to_ascii_lowercase()).collect()
    }

    #[test]
    fn exact_host_matches() {
        let s = host_set(&["example.net"]);
        assert!(is_host_allowed("https://example.net/path", &s));
        assert!(!is_host_allowed("https://evil.com/path", &s));
    }

    #[test]
    fn subdomain_wildcard() {
        let s = host_set(&["*.example.net"]);
        assert!(is_host_allowed("https://cdn.example.net/a.png", &s));
        assert!(is_host_allowed("https://deeply.nested.example.net/x", &s));
        assert!(!is_host_allowed("https://example.net/x", &s));
        assert!(!is_host_allowed("https://evil.com/x", &s));
    }

    #[test]
    fn brace_alternation() {
        let s = host_set(&["{cdn,static}.example.net"]);
        assert!(is_host_allowed("https://cdn.example.net/x", &s));
        assert!(is_host_allowed("https://static.example.net/x", &s));
        assert!(!is_host_allowed("https://media.example.net/x", &s));
    }

    #[test]
    fn case_insensitive_host() {
        let s = host_set(&["example.net"]);
        assert!(is_host_allowed("https://EXAMPLE.NET/path", &s));
        assert!(is_host_allowed("HTTPS://Example.Net/path", &s));
    }

    #[test]
    fn port_is_ignored() {
        let s = host_set(&["example.net"]);
        assert!(is_host_allowed("https://example.net:8443/x", &s));
    }

    #[test]
    fn no_host_passes_through() {
        let s = host_set(&["example.net"]);
        assert!(is_host_allowed("/local/path", &s));
        assert!(is_host_allowed("relative.html", &s));
        assert!(is_host_allowed("#anchor", &s));
        assert!(is_host_allowed("mailto:user@example.net", &s));
        assert!(is_host_allowed("tel:+1234567890", &s));
        assert!(is_host_allowed("javascript:alert(1)", &s));
        assert!(is_host_allowed("", &s));
    }

    #[test]
    fn empty_host_allowlist_blocks_all_external() {
        let s = host_set(&[]);
        assert!(!is_host_allowed("https://example.net/x", &s));
        assert!(!is_host_allowed("https://anything.com/x", &s));
        assert!(is_host_allowed("/local", &s));
    }

    #[test]
    fn scheme_matches_allowed() {
        let s = scheme_set(&["http", "https", "mailto"]);
        assert!(is_scheme_allowed("https://example.net/x", &s));
        assert!(is_scheme_allowed("http://example.net/x", &s));
        assert!(is_scheme_allowed("mailto:user@example.net", &s));
    }

    #[test]
    fn scheme_rejects_disallowed() {
        let s = scheme_set(&["http", "https", "mailto"]);
        assert!(!is_scheme_allowed("javascript:alert(1)", &s));
        assert!(!is_scheme_allowed("vbscript:msgbox", &s));
        assert!(!is_scheme_allowed("data:text/html,<script>", &s));
        assert!(!is_scheme_allowed("file:///etc/passwd", &s));
        assert!(!is_scheme_allowed("tel:+1234567890", &s));
    }

    #[test]
    fn scheme_is_case_insensitive_via_url_crate() {
        // url::Url lowercases the scheme at parse time, so mixed-case
        // input matches lowercase allowlist entries.
        let s = scheme_set(&["https"]);
        assert!(is_scheme_allowed("HTTPS://example.net", &s));
        assert!(is_scheme_allowed("HttpS://example.net", &s));
    }

    #[test]
    fn unparseable_url_passes_scheme_check() {
        // Relative, anchor-only, protocol-relative, and empty URLs can't
        // be parsed as absolute—no scheme to check, so they pass.
        let s = scheme_set(&["https"]);
        assert!(is_scheme_allowed("/local/path", &s));
        assert!(is_scheme_allowed("relative.html", &s));
        assert!(is_scheme_allowed("#anchor", &s));
        assert!(is_scheme_allowed("//cdn.example.net/x", &s));
        assert!(is_scheme_allowed("", &s));
    }

    #[test]
    fn empty_scheme_allowlist_blocks_all_absolute() {
        let s = scheme_set(&[]);
        assert!(!is_scheme_allowed("https://example.net", &s));
        assert!(!is_scheme_allowed("mailto:user@example.net", &s));
        // Relative URLs still pass—nothing to match.
        assert!(is_scheme_allowed("/local", &s));
    }
}
