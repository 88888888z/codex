# Configuration

For basic configuration instructions, see [this documentation](https://developers.openai.com/codex/config-basic).

For advanced configuration instructions, see [this documentation](https://developers.openai.com/codex/config-advanced).

For a full configuration reference, see [this documentation](https://developers.openai.com/codex/config-reference).

## TUI model cost estimates

The fullscreen inspector can estimate session cost from token usage. Configure
prices per model in the selected currency per one million tokens:

```toml
[tui.model_pricing."gpt-5.4"]
cache_hit = 0.25
cache_miss = 1.25
output = 10.00
peak_multiplier = 1.0
currency = "$"
```

The key must match the active model slug. When no price is configured for that
model, the inspector still shows token and cache usage but omits the cost row.
For DeepSeek, set `peak_multiplier = 2.0`; the multiplier applies on weekdays
from 09:00–12:00 and 14:00–18:00 Beijing time. Public-holiday calendars are not
detected automatically. The inspector applies the current billing window to the
whole session estimate, so the result is an approximation.

## Lifecycle hooks

Admins can set top-level `allow_managed_hooks_only = true` in
`requirements.toml` to ignore user, project, and session hook configs while
still allowing managed hooks from requirements and managed config layers. This
setting is only supported in `requirements.toml`; putting it in `config.toml`
does not enable managed-hooks-only mode.
