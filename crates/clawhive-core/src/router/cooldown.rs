use std::collections::HashMap;
use std::time::{Duration, Instant};

pub(crate) const DEFAULT_COOLDOWN_SECS: u64 = 60;
pub(crate) const BILLING_COOLDOWN_SECS: u64 = 300; // 5 minutes for billing errors

/// Tracks cooldown state for providers (similar to OpenClaw's auth profile stats)
#[derive(Debug, Clone, Default)]
pub struct ProviderCooldownStats {
    pub cooldown_until: Option<Instant>,
    pub failure_count: u32,
    pub last_failure_reason: Option<String>,
}

impl ProviderCooldownStats {
    pub fn is_in_cooldown(&self) -> bool {
        self.cooldown_until
            .map(|until| Instant::now() < until)
            .unwrap_or(false)
    }

    pub fn remaining_cooldown(&self) -> Option<Duration> {
        self.cooldown_until.and_then(|until| {
            let now = Instant::now();
            if now < until {
                Some(until - now)
            } else {
                None
            }
        })
    }

    pub fn set_cooldown(&mut self, duration: Duration, reason: &str) {
        self.cooldown_until = Some(Instant::now() + duration);
        self.failure_count += 1;
        self.last_failure_reason = Some(reason.to_string());
    }

    pub fn clear_cooldown(&mut self) {
        self.cooldown_until = None;
    }
}

/// Cooldown store for all providers
#[derive(Debug, Clone, Default)]
pub struct CooldownStore {
    stats: HashMap<String, ProviderCooldownStats>,
}

impl CooldownStore {
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
        }
    }

    pub fn get_stats(&self, provider_id: &str) -> ProviderCooldownStats {
        self.stats.get(provider_id).cloned().unwrap_or_default()
    }

    pub fn set_cooldown(&mut self, provider_id: &str, duration: Duration, reason: &str) {
        let stats = self.stats.entry(provider_id.to_string()).or_default();
        stats.set_cooldown(duration, reason);
    }

    pub fn clear_cooldown(&mut self, provider_id: &str) {
        if let Some(stats) = self.stats.get_mut(provider_id) {
            stats.clear_cooldown();
        }
    }

    pub fn clear_expired_cooldowns(&mut self) {
        let now = Instant::now();
        for stats in self.stats.values_mut() {
            if let Some(until) = stats.cooldown_until {
                if now >= until {
                    stats.cooldown_until = None;
                }
            }
        }
    }
}
