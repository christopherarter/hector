# Quickstart

Create a `.hector.yml` in your repo root:

```yaml
schema_version: 2

rules:
  no-debug:
    description: "no DEBUG markers in source"
    engine: script
    scope: ["src/**/*"]
    severity: error
    script: "grep -nE 'DEBUG' {file} && exit 1 || exit 0"
```

Trust it (review the config first — `hector` runs the scripts in it):

```bash
hector trust
```

Run check against a file:

```bash
hector check --file src/foo.rs
```

Exit codes:

- `0` — pass (or warnings only)
- `1` — internal error (config invalid, untrusted)
- `2` — at least one error-severity violation
