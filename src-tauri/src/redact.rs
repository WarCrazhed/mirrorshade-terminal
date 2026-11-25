//! Secret redaction applied to everything before it is sent to an LLM, and
//! used to keep secrets out of the local history store.
//!
//! Pattern set is modeled on gitleaks' high-signal rules plus generic
//! `key=value` assignments and the user's home path. False positives are fine
//! (we only lose some context); false negatives are what we try to avoid.

use regex::Regex;
use std::sync::LazyLock;

struct Rule {
    re: Regex,
    /// Capture group holding the secret (0 = whole match).
    group: usize,
    label: &'static str,
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let r = |pat: &str, group: usize, label: &'static str| Rule {
        re: Regex::new(pat).expect("valid redaction regex"),
        group,
        label,
    };
    vec![
        r(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)", 0, "PRIVATE_KEY"),
        r(r"\bsk-ant-[A-Za-z0-9_\-]{20,}", 0, "ANTHROPIC_KEY"),
        r(r"\bsk-(?:proj-)?[A-Za-z0-9_\-]{20,}", 0, "API_KEY"),
        r(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b", 0, "AWS_KEY"),
        r(r"\bgh[pousr]_[A-Za-z0-9]{30,}\b", 0, "GITHUB_TOKEN"),
        r(r"\bgithub_pat_[A-Za-z0-9_]{40,}\b", 0, "GITHUB_TOKEN"),
        r(r"\bglpat-[A-Za-z0-9_\-]{20,}\b", 0, "GITLAB_TOKEN"),
        r(r"\bxox[abposr]-[A-Za-z0-9-]{10,}\b", 0, "SLACK_TOKEN"),
        r(r"\bAIza[0-9A-Za-z_\-]{35}\b", 0, "GOOGLE_KEY"),
        r(r"\b(?:sk|rk|pk)_(?:live|test)_[A-Za-z0-9]{20,}\b", 0, "STRIPE_KEY"),
        r(r"\beyJ[A-Za-z0-9_\-]{8,}\.eyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}", 0, "JWT"),
        // Credentials inside URLs: scheme://user:password@host
        r(r"([a-zA-Z][a-zA-Z0-9+.\-]*://[^\s:/@]+:)([^\s@/]+)(@)", 2, "PASSWORD"),
        // Authorization headers
        r(r"(?i)(authorization:\s*(?:bearer|basic|token)\s+)([^\s'\x22]+)", 2, "TOKEN"),
        // KEY=value / key: value where the key name smells secret
        r(
            r#"(?i)((?:^|[\s'"{,;(])[A-Za-z0-9_.\-]*(?:pass(?:word|wd)?|secret|token|api[_\-]?key|access[_\-]?key|private[_\-]?key|auth|credential|client[_\-]?secret|session)[A-Za-z0-9_.\-]*['"]?\s*[:=]\s*['"]?)([^\s'",;)}]{4,})"#,
            2,
            "SECRET",
        ),
        // CLI flags: --password foo, -p=foo (mysql style is -pfoo; skip, too ambiguous)
        r(r"(?i)(--(?:password|passwd|token|secret|api-key|apikey)[= ]\s*)(\S+)", 2, "SECRET"),
    ]
});

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Redacted {
    pub text: String,
    pub count: usize,
}

/// Replace secrets with `[REDACTED:LABEL]` and the home directory with `~`.
pub fn redact(input: &str) -> Redacted {
    let mut text = input.to_string();
    let mut count = 0;
    for rule in RULES.iter() {
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for caps in rule.re.captures_iter(&text) {
            let Some(m) = caps.get(rule.group) else { continue };
            // Don't re-redact our own placeholders.
            if m.as_str().starts_with("[REDACTED") {
                continue;
            }
            out.push_str(&text[last..m.start()]);
            out.push_str("[REDACTED:");
            out.push_str(rule.label);
            out.push(']');
            last = m.end();
            count += 1;
        }
        out.push_str(&text[last..]);
        text = out;
    }
    if let Some(home) = std::env::var_os("HOME").and_then(|h| h.into_string().ok()) {
        if home.len() > 1 {
            text = text.replace(&home, "~");
        }
    }
    Redacted { text, count }
}

pub fn contains_secret(input: &str) -> bool {
    RULES.iter().any(|r| r.re.is_match(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red(s: &str) -> String {
        redact(s).text
    }

    #[test]
    fn redacts_known_token_formats() {
        // All fake, shaped like real keys.
        let cases = [
            "export ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuvwxyz012345",
            "aws AKIAIOSFODNN7EXAMPLE configured",
            "git clone https://ghp_abcdefghijklmnopqrstuvwxyz0123456789@github.com/x/y",
            "token eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
            "curl -H 'Authorization: Bearer abc123def456ghi789'",
            "mysql://root:hunter2pass@localhost/db",
            "DB_PASSWORD=supersecret123",
            // Split so secret scanners (GitHub push protection) don't flag the fake key.
            concat!("stripe sk_", "live_abcdefghijklmnopqrstuvwx"),
        ];
        for c in cases {
            let r = redact(c);
            assert!(r.count > 0, "no redaction in: {c}");
            assert!(r.text.contains("[REDACTED:"), "{c} -> {}", r.text);
        }
        assert!(!red(cases[0]).contains("abcdefghijklmnop"));
        assert!(!red(cases[5]).contains("hunter2pass"));
        assert!(red(cases[5]).contains("mysql://root:"));
        assert!(!red(cases[6]).contains("supersecret123"));
    }

    #[test]
    fn redacts_private_keys() {
        let k = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE KEY-----";
        let r = redact(k);
        assert_eq!(r.text, "[REDACTED:PRIVATE_KEY]");
    }

    #[test]
    fn leaves_normal_output_alone() {
        let s = "error[E0308]: mismatched types\n  --> src/main.rs:4:5\nls -la /usr/bin";
        let r = redact(s);
        assert_eq!(r.count, 0);
        assert_eq!(r.text, s);
        assert!(!contains_secret("git commit -m 'fix token parsing'"));
    }

    #[test]
    fn replaces_home_dir() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(red(&format!("cd {home}/proj")), "cd ~/proj");
    }
}
