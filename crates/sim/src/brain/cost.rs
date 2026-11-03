//! Session-local accounting, independent of providers and persistent boss memory.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CostTotals {
    pub calls: u64,
    /// Input includes cached input; output includes reasoning. Never add those subsets twice.
    pub input: u64,
    pub cached: u64,
    pub output: u64,
    pub reasoning: u64,
    pub billed_usd: f64,
    pub plan_tokens: u64,
    pub api_tokens: u64,
    pub unknown_billed: u64,
}

impl CostTotals {
    pub fn tokens(&self) -> u64 {
        self.input.saturating_add(self.output)
    }
    pub fn since(&self, old: &Self) -> Self {
        Self {
            calls: self.calls.saturating_sub(old.calls),
            input: self.input.saturating_sub(old.input),
            cached: self.cached.saturating_sub(old.cached),
            output: self.output.saturating_sub(old.output),
            reasoning: self.reasoning.saturating_sub(old.reasoning),
            billed_usd: (self.billed_usd - old.billed_usd).max(0.0),
            plan_tokens: self.plan_tokens.saturating_sub(old.plan_tokens),
            api_tokens: self.api_tokens.saturating_sub(old.api_tokens),
            unknown_billed: self.unknown_billed.saturating_sub(old.unknown_billed),
        }
    }
    pub fn line(&self, label: &str) -> String {
        let mut line = format!("{label}: {} calls", self.calls);
        if self.plan_tokens > 0 {
            line.push_str(&format!(
                " · {} tok (plan proxy)",
                compact(self.plan_tokens)
            ));
        }
        if self.api_tokens > 0 || self.unknown_billed > 0 || self.billed_usd > 0.0 {
            line.push_str(&format!(
                " · {} tok · {} (API)",
                compact(self.api_tokens),
                usd(self.billed_usd, self.unknown_billed)
            ));
        }
        line
    }
}

fn compact(n: u64) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

pub fn usd(value: f64, unknown: u64) -> String {
    if unknown > 0 {
        format!("unknown (${value:.6} known)")
    } else {
        format!("${value:.6}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct CostRow {
    pub kind: String,
    pub model: String,
    pub totals: CostTotals,
}

#[derive(Debug, Clone, Default)]
pub struct CostSnapshot {
    pub totals: CostTotals,
    pub rows: Vec<CostRow>,
    pub budget_notice: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_session_line_and_fight_delta_count_subsets_once() {
        let old = CostTotals {
            calls: 1,
            input: 10000,
            cached: 2000,
            output: 1000,
            plan_tokens: 11000,
            ..Default::default()
        };
        let now = CostTotals {
            calls: 12,
            input: 47000,
            cached: 2000,
            output: 3000,
            reasoning: 500,
            plan_tokens: 48000,
            api_tokens: 2000,
            billed_usd: 0.0002,
            ..Default::default()
        };
        assert_eq!(now.tokens(), 50000);
        assert_eq!(
            now.line("session"),
            "session: 12 calls · 48.0k tok (plan proxy) · 2.0k tok · $0.000200 (API)"
        );
        let fight = now.since(&old);
        assert_eq!(fight.calls, 11);
        assert_eq!(fight.tokens(), 39000);
        assert_eq!(fight.plan_tokens, 37000);
        assert_eq!(fight.api_tokens, 2000);
    }
}
