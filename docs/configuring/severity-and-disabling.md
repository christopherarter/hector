# Severity and disabling rules

Severity decides whether a violation blocks an edit or only reports it. Disable directives let you wave through a specific line that you've decided is fine.

## Severity

Every rule sets `severity:` to `error` or `warning`:

```yaml
rules:
  no-todo-in-src:
    description: "todo!() must not ship."
    engine: ast
    language: rust
    scope: ["crates/*/src/**/*.rs"]
    severity: error      # blocks the edit

  no-unwrap-in-src:
    description: "Prefer ? over .unwrap() in production source."
    engine: ast
    language: rust
    scope: ["crates/*/src/**/*.rs"]
    severity: warning    # reports, but allows
```

The two map straight onto the verdict and exit code:

| Severity | Verdict if it fires | `hector check` exit | Adapter behavior |
|----------|--------------------|--------------------|-------------------|
| `error` | `block` | `2` | Rejects the edit; the agent retries. |
| `warning` | `warn` | `0` | Allows the edit; surfaces the warning. |

A check's overall status is the worst of its rules: any `error`-severity hit blocks; otherwise any `warning` hit warns; otherwise it passes. See [Verdict JSON](../reference/verdict-json.md) for the exact shape.

Start a new rule at `warning`. Once you trust it not to misfire, promote it to `error`.

## Disabling a rule in-line

Sometimes a specific occurrence is legitimately fine. Add a `hector-disable:` directive on that line — usually in a comment — naming the rule id to suppress:

```rust
let port = env::var("PORT").unwrap(); // hector-disable: no-unwrap-in-src
```

The directive suppresses only the named rule, and only where it applies:

- For violations with a line number (AST matches), it suppresses the rule **on that line**.
- For file-level violations with no line (most `script` and `semantic` findings), a directive anywhere in the file suppresses that rule **file-wide**.

### Naming rules

One rule id per directive. To disable two rules on the same line, write two directives:

```python
x = eval(s)  # hector-disable: no-eval  hector-disable: py/no-dynamic
```

The directive reads the rule id up to whitespace, a comma, or a comment terminator. Namespaced ids with slashes (`python/no-print`) are preserved — only `//` and `/*` end the id, so a bare `/` inside an id is safe.

## Disabling vs. baselining

Two different tools for "don't flag this":

- **`hector-disable:`** is a deliberate, in-source exception for a specific line you've reviewed. It lives in the code and travels with it.
- **Baselining** silences a batch of *pre-existing* violations you haven't fixed yet, so a new rule doesn't drown you in noise on day one. It lives in `.hector/baseline.json`.

Reach for a disable directive when a single occurrence is correct by design. Reach for a baseline when you're adopting a rule on a codebase that doesn't pass it yet. See [Baselines](baselines.md).

## See also

- [Baselines](baselines.md) — silence pre-existing violations in bulk
- [Verdict JSON](../reference/verdict-json.md) — how severity maps to status and exit code
- [Running checks](../operating/running-checks.md) — the exit-code contract adapters rely on
