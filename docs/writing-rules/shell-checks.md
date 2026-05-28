# Running a shell check

The `script` engine runs a shell command against each file in scope. If the command exits non-zero, the rule fires. This is the engine to reach for when a grep, a linter, or a one-off script can already answer the question.

```yaml
rules:
  no-console-log:
    description: "No console.log in committed source."
    engine: script
    scope: ["src/**/*.ts"]
    severity: error
    script: "grep -nE 'console\\.log\\(' {file} && exit 1 || exit 0"
```

When `hector check` runs this against a `.ts` file, the command runs with `{file}` pointing at the path. `grep` exits `0` when it finds a match, so the rule forces `exit 1` on a hit and `exit 0` otherwise. Exit non-zero means "violation"; exit zero means "clean".

## The `{file}` token

`{file}` expands to the path under check. Hector passes the path through the environment, not by splicing it into the command text, so a filename with spaces or shell metacharacters can't break out of the command. You can use `{file}` as many times as you need.

Under the hood `{file}` becomes `"$HECTOR_FILE"`, and the path is also available directly as the `HECTOR_FILE` environment variable if you'd rather reference it that way in a longer script.

## Exit code is the signal

The contract is the exit code, nothing else:

- **Exit `0`** — no violation. The rule passes regardless of what the command printed.
- **Any non-zero exit** — the rule fired. Hector turns the command's output into a violation message.

Write your `script:` so a clean file exits `0`. Wrappers like `&& exit 1 || exit 0` invert tools (such as `grep`) whose natural exit codes don't match that contract.

## What the agent sees: `passthrough` vs `parsed`

When a rule fires, Hector has to turn the command's output into a violation message. Two modes control how:

### `passthrough` (default)

The command's stdout and stderr are kept verbatim as the violation message, with no line number attached. This is the default because it never mangles output — a linter that already prints pretty, framed diagnostics keeps them intact.

```yaml
rules:
  rustfmt-check:
    description: "Run rustfmt and surface any diff."
    engine: script
    scope: ["src/**/*.rs"]
    severity: warning
    script: "rustfmt --check {file}"
```

Whatever `rustfmt --check` prints lands in the violation as-is.

### `parsed`

Opt in with `output: parsed` when your command emits machine-readable diagnostics and you want one violation per finding, each with its own file, line, and column:

```yaml
rules:
  ruff:
    description: "Ruff lint findings."
    engine: script
    scope: ["**/*.py"]
    severity: error
    output: parsed
    script: "ruff check --output-format concise {file}"
```

Parsed mode understands a fixed set of shapes:

- `file:line:col: message` (ruff, `eslint --format compact`, `clippy --message-format short`)
- `grep -n` output (`line:text`)
- JSON objects and arrays

That set is deliberately fixed — Hector won't grow a bespoke parser per linter. If your tool's format isn't on the list, stay on `passthrough`.

## Sandboxing a script

A `script:` rule can declare the capabilities it needs. By default a rule gets no network and no writes outside its working directory:

```yaml
rules:
  no-console-log:
    description: "No console.log in committed source."
    engine: script
    scope: ["src/**/*.ts"]
    severity: error
    script: "grep -nE 'console\\.log\\(' {file} && exit 1 || exit 0"
    capabilities:
      network: false
      writes: cwd-only
```

Network isolation is enforced on Linux and advisory on macOS; the writes policy is advisory in the current release. The capability sandbox protects against a *misconfigured* script, not a malicious one — the [trust gate](../security/trust.md) is the defense against malicious rules. Read [Capability sandboxing](../security/capabilities.md) before relying on it.

## A note for pre-edit adapters

`script:` rules run their command against the file **on disk**. Adapters that gate a proposed edit *before* it lands (via `hector check --content`) will have the script see the current disk content, not the proposed content. AST and semantic rules read the proposed content correctly. If you gate pre-edit, prefer `ast` or `semantic` for the rules that must see the new bytes.

## See also

- [Matching code structure](matching-code.md) — when a structural pattern beats a regex
- [Capability sandboxing](../security/capabilities.md) — what `capabilities:` actually enforces
- [Config schema](../reference/config-schema.md#rule) — every `script` rule field
