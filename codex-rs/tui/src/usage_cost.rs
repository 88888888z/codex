//! Session cost accumulation from token-usage deltas and model price config.

use crate::token_usage::TokenUsage;
use chrono::Datelike;
use chrono::FixedOffset;
use chrono::Timelike;
use chrono::Utc;
use chrono::Weekday;
use codex_config::types::TuiModelPricing;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct ModelCostEstimate {
    pub(crate) amount: f64,
    pub(crate) currency: String,
}

#[derive(Default)]
pub(crate) struct SessionUsageCost {
    previous: UsageTotals,
    models: BTreeMap<String, CostBuckets>,
}

#[derive(Clone, Copy, Default)]
struct UsageTotals {
    input: i64,
    cached: i64,
    output: i64,
}

#[derive(Clone, Copy, Default)]
struct CostCounts {
    cache_hit: i64,
    cache_miss: i64,
    output: i64,
}

#[derive(Clone, Copy, Default)]
struct CostBuckets {
    peak: CostCounts,
    idle: CostCounts,
}

impl SessionUsageCost {
    /// Add the new portion of cumulative provider usage to the current model and billing period.
    pub(crate) fn observe(&mut self, total: &TokenUsage, model: &str) {
        if total.input_tokens < self.previous.input
            || total.cached_input_tokens < self.previous.cached
            || total.output_tokens < self.previous.output
        {
            self.previous = UsageTotals::default();
            self.models.clear();
        }
        let input = total
            .input_tokens
            .saturating_sub(self.previous.input)
            .max(0);
        let cached = total
            .cached_input_tokens
            .saturating_sub(self.previous.cached)
            .max(0)
            .min(input);
        let output = total
            .output_tokens
            .saturating_sub(self.previous.output)
            .max(0);
        self.previous = UsageTotals {
            input: total.input_tokens,
            cached: total.cached_input_tokens,
            output: total.output_tokens,
        };
        if input == 0 && output == 0 {
            return;
        }
        let bucket = self.models.entry(model.to_string()).or_default();
        let counts = if is_beijing_peak_time() {
            &mut bucket.peak
        } else {
            &mut bucket.idle
        };
        counts.cache_hit = counts.cache_hit.saturating_add(cached);
        counts.cache_miss = counts
            .cache_miss
            .saturating_add(input.saturating_sub(cached));
        counts.output = counts.output.saturating_add(output);
    }

    pub(crate) fn estimate(
        &self,
        pricing: &BTreeMap<String, TuiModelPricing>,
    ) -> Option<ModelCostEstimate> {
        let mut amount = 0.0;
        let mut currency: Option<String> = None;
        for (model, buckets) in &self.models {
            let price = pricing.get(model)?;
            let model_currency = if price.currency.trim().is_empty() {
                "$".to_string()
            } else {
                price.currency.clone()
            };
            if currency
                .as_ref()
                .is_some_and(|configured| configured != &model_currency)
            {
                return None;
            }
            currency = Some(model_currency);
            amount += cost_for_bucket(buckets.idle, price, /*multiplier*/ 1.0);
            amount += cost_for_bucket(buckets.peak, price, normalized_price(price.peak_multiplier));
        }
        currency.map(|currency| ModelCostEstimate { amount, currency })
    }
}

fn cost_for_bucket(counts: CostCounts, pricing: &TuiModelPricing, multiplier: f64) -> f64 {
    (counts.cache_hit.max(0) as f64 * normalized_price(pricing.cache_hit)
        + counts.cache_miss.max(0) as f64 * normalized_price(pricing.cache_miss)
        + counts.output.max(0) as f64 * normalized_price(pricing.output))
        * multiplier
        / 1_000_000.0
}

fn normalized_price(price: f64) -> f64 {
    if price.is_finite() {
        price.max(0.0)
    } else {
        0.0
    }
}

fn is_beijing_peak_time() -> bool {
    let Some(offset) = FixedOffset::east_opt(8 * 60 * 60) else {
        return false;
    };
    is_peak_datetime(&Utc::now().with_timezone(&offset))
}

fn is_peak_datetime(now: &chrono::DateTime<FixedOffset>) -> bool {
    if matches!(now.weekday(), Weekday::Sat | Weekday::Sun) {
        return false;
    }
    let seconds = now.num_seconds_from_midnight();
    (9 * 60 * 60..12 * 60 * 60).contains(&seconds)
        || (14 * 60 * 60..18 * 60 * 60).contains(&seconds)
}
