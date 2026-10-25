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

## Multi-project workspaces

Place `.codex/workspace.toml` in the parent folder that contains several
projects. Codex discovers the nearest manifest from the launch directory.
The former root-level `.codex-workspace.toml` remains readable as a legacy
fallback:

```toml
name = "company"

[[roots]]
id = "web"
path = "./web"
write = true

[[roots]]
id = "api"
path = "./api"
write = true
```

Every root is exposed to the model as `@web`, `@api`, and so on. Roots marked
`write = true` are added to the session's writable workspace roots. Relative
paths resolve against the manifest directory. Use `/projects` to inspect the
resolved roots and open a root in the file browser. Edit the manifest and
restart Codex to change the scope. `/verify` discovers and runs checks for
every project root and prefixes each result with its `@id`.

If no manifest exists, `/projects` scans direct child directories and opens a
multi-select list. Confirming the selection creates `.codex/workspace.toml` in
the current directory with every selected project marked writable. Hidden
directories and common build directories are omitted.

When Codex starts from the manifest's parent directory, the first writable root
becomes the primary working directory so the parent folder itself is not the
write root. Starting inside a child project keeps that project as the primary
working directory unless `--cd` is supplied.

Sibling directories that are not listed remain outside the project map. Under
danger-full-access the `write` flag is advisory; the model is instructed not to
modify read-only roots.

## Lifecycle hooks

Admins can set top-level `allow_managed_hooks_only = true` in
`requirements.toml` to ignore user, project, and session hook configs while
still allowing managed hooks from requirements and managed config layers. This
setting is only supported in `requirements.toml`; putting it in `config.toml`
does not enable managed-hooks-only mode.
