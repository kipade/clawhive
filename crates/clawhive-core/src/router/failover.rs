use std::time::Duration;

use super::cooldown::{BILLING_COOLDOWN_SECS, DEFAULT_COOLDOWN_SECS};

/// Error classification for failover decisions (following OpenClaw patterns)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverReason {
    RateLimit,       // 429
    Billing,         // insufficient credits / billing error
    Timeout,         // request timed out
    ServerError,     // 5xx
    ContextOverflow, // context too long
    AuthError,       // 401/403
    Unknown,         // other errors
}

impl FailoverReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RateLimit => "rate_limit",
            Self::Billing => "billing",
            Self::Timeout => "timeout",
            Self::ServerError => "server_error",
            Self::ContextOverflow => "context_overflow",
            Self::AuthError => "auth_error",
            Self::Unknown => "unknown",
        }
    }
}

/// Classify an error to determine if/how to failover
pub fn classify_failover_reason(err_str: &str) -> Option<FailoverReason> {
    let lower = err_str.to_lowercase();

    // Rate limit (429)
    if lower.contains("429") || lower.contains("rate limit") || lower.contains("rate_limit") {
        return Some(FailoverReason::RateLimit);
    }

    // Billing errors
    if lower.contains("insufficient")
        || lower.contains("billing")
        || lower.contains("credits")
        || lower.contains("quota")
        || lower.contains("payment")
    {
        return Some(FailoverReason::Billing);
    }

    // Timeout
    if lower.contains("timeout") || lower.contains("timed out") || lower.contains("deadline") {
        return Some(FailoverReason::Timeout);
    }

    // Server errors (5xx)
    if lower.contains("500")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
        || lower.contains("internal server error")
        || lower.contains("service unavailable")
        || lower.contains("bad gateway")
    {
        return Some(FailoverReason::ServerError);
    }

    // Context overflow
    if lower.contains("context")
        && (lower.contains("overflow")
            || lower.contains("too long")
            || lower.contains("too large")
            || lower.contains("exceed"))
    {
        return Some(FailoverReason::ContextOverflow);
    }

    // Auth errors (401/403)
    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("authentication")
    {
        return Some(FailoverReason::AuthError);
    }

    // Check for [retryable] marker - if present but not caught above, it's still failover-eligible
    if lower.contains("[retryable]") {
        return Some(FailoverReason::Unknown);
    }

    None
}

/// Check if an error should trigger failover
#[allow(dead_code)]
pub fn is_failover_error(err_str: &str) -> bool {
    classify_failover_reason(err_str).is_some()
}

/// Get cooldown duration based on error type
pub fn get_cooldown_duration(reason: FailoverReason) -> Duration {
    match reason {
        FailoverReason::Billing => Duration::from_secs(BILLING_COOLDOWN_SECS),
        FailoverReason::RateLimit => Duration::from_secs(DEFAULT_COOLDOWN_SECS),
        FailoverReason::Timeout => Duration::from_secs(30),
        FailoverReason::ServerError => Duration::from_secs(30),
        FailoverReason::ContextOverflow => Duration::from_secs(0), // no cooldown, just skip
        FailoverReason::AuthError => Duration::from_secs(3600),    // 1 hour for auth issues
        FailoverReason::Unknown => Duration::from_secs(DEFAULT_COOLDOWN_SECS),
    }
}
