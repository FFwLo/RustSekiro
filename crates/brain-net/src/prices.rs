use crate::{ConfigError, TokenUsage};

#[derive(Debug, Clone)]
struct Price {
    model: String,
    operation: String,
    input: f64,
    cached: f64,
    output: f64,
    reasoning: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_count_cached_and_reasoning_subsets_once_and_unknowns_are_absent() {
        let prices = PriceTable::default();
        let usage = TokenUsage {
            input_tokens: 10000,
            cached_input_tokens: 2000,
            output_tokens: 1000,
            reasoning_tokens: 600,
        };
        assert_eq!(prices.estimate("missing", "responses", usage), None);
        assert!(
            (prices.estimate("gpt-6.1-sol", "responses", usage).unwrap() - 0.0262).abs() < 1e-12
        );
        assert!((prices.estimate("gpt-6-luna", "decisions", usage).unwrap() - 0.001).abs() < 1e-12);
        let text = include_str!("../brain-prices.toml")
            .replace("output = 10.00", "output = 10.00\nreasoning = 20.00");
        assert!(
            (PriceTable::parse(&text)
                .unwrap()
                .estimate("gpt-6.1-sol", "responses", usage)
                .unwrap()
                - 0.0322)
                .abs()
                < 1e-12
        );
        assert!(PriceTable::parse(&text.replace("input = 2.00", "input = -1.00")).is_err());
    }
}

#[derive(Debug, Clone)]
pub struct PriceTable {
    prices: Vec<Price>,
}

impl Default for PriceTable {
    fn default() -> Self {
        Self::parse(include_str!("../brain-prices.toml")).expect("bundled prices")
    }
}

impl PriceTable {
    /// An absent file uses the bundled table; a malformed file fails closed.
    pub fn load() -> Result<Self, ConfigError> {
        match std::fs::read_to_string("config/brain-prices.toml") {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err(ConfigError::InvalidConfig),
        }
    }
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|_| ConfigError::InvalidConfig)?;
        // Billing classification comes from authentication, not an editable switch.
        for backend in ["codex", "claude"] {
            if doc["backends"][backend]["subscription"].as_bool() != Some(true) {
                return Err(ConfigError::InvalidConfig);
            }
        }
        let rows = doc["prices"]
            .as_array_of_tables()
            .ok_or(ConfigError::InvalidConfig)?;
        let mut prices = Vec::new();
        for row in rows {
            let number = |key: &str| -> Result<f64, ConfigError> {
                let n = row
                    .get(key)
                    .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)))
                    .ok_or(ConfigError::InvalidConfig)?;
                if n.is_finite() && n >= 0.0 {
                    Ok(n)
                } else {
                    Err(ConfigError::InvalidConfig)
                }
            };
            let string = |key: &str| {
                row.get(key)
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .ok_or(ConfigError::InvalidConfig)
            };
            let p = Price {
                model: string("model")?,
                operation: string("operation")?,
                input: number("input")?,
                cached: number("cached_input")?,
                output: number("output")?,
                reasoning: row
                    .contains_key("reasoning")
                    .then(|| number("reasoning"))
                    .transpose()?,
            };
            if prices
                .iter()
                .any(|old: &Price| old.model == p.model && old.operation == p.operation)
            {
                return Err(ConfigError::InvalidConfig);
            }
            prices.push(p);
        }
        Ok(Self { prices })
    }
    pub fn estimate(&self, model: &str, operation: &str, u: TokenUsage) -> Option<f64> {
        let p = self
            .prices
            .iter()
            .find(|p| p.model == model && p.operation == operation)?;
        let cached = u.cached_input_tokens.min(u.input_tokens);
        let reasoning = u.reasoning_tokens.min(u.output_tokens);
        let output = match p.reasoning {
            Some(rate) => (u.output_tokens - reasoning) as f64 * p.output + reasoning as f64 * rate,
            None => u.output_tokens as f64 * p.output,
        };
        Some(
            ((u.input_tokens - cached) as f64 * p.input + cached as f64 * p.cached + output)
                / 1_000_000.0,
        )
    }
}
