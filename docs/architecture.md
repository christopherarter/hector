# Architecture diagram

Hector turns repo-local policy into an automatic gate for AI coding agents. The short version: adapters catch edits, the `hector` binary checks those edits against trusted rules, and the adapter turns the verdict back into "keep going" or "fix this first."

```mermaid
flowchart LR
    subgraph People["People and policy"]
        Team["Team intent<br/>security, style, tests, architecture"]
        Config[".hector.yml<br/>rules, scope, severity"]
        Trust["Trust fingerprint<br/>reviewed config before rules run"]
        Trusted["Trusted resolved config<br/>extends merged, fingerprint verified"]
        Baseline["Baseline and disables<br/>suppress known or approved findings"]
    end

    subgraph Agents["AI coding tools"]
        Claude["Claude Code"]
        OpenCode["OpenCode"]
        Reasonix["Reasonix"]
        Pi["pi"]
        Future["Future and custom adapters<br/>Aider, pre-commit, MCP"]
    end

    subgraph AdapterLayer["Adapter layer"]
        Hooks["Edit hooks<br/>capture proposed content or diff"]
        Contract["Stable command contract<br/>hector check --format json"]
    end

    subgraph Hector["Hector"]
        CLI["hector CLI<br/>arguments, I/O, exit codes"]
        Core["hector-core pipeline<br/>load config, verify trust, match scope"]

        subgraph Engines["Two rule engines"]
            Script["script<br/>run project checks and linters"]
            AST["ast<br/>match code structure"]
        end

        Filter["Noise control<br/>baseline and hector-disable filters"]
        Verdict["Verdict JSON<br/>pass, warn, block, or internal_error"]
        Telemetry["Telemetry<br/>append-only check log"]
    end

    subgraph Outcome["Outcome"]
        Allow["Allow edit<br/>agent continues"]
        Warn["Warn<br/>surface policy feedback"]
        Block["Block edit<br/>adapter rejects the edit so the agent retries"]
        Audit["Operate and improve<br/>review noisy, dead, or valuable rules"]
    end

    Team --> Config
    Config --> Trust
    Trust --> Trusted
    Trusted --> Core
    Baseline --> Filter

    Claude --> Hooks
    OpenCode --> Hooks
    Reasonix --> Hooks
    Pi --> Hooks
    Future --> Hooks
    Hooks --> Contract
    Contract --> CLI
    CLI --> Core

    Core --> Script
    Core --> AST
    Script --> Filter
    AST --> Filter
    Filter --> Verdict
    Verdict --> Telemetry
    Verdict --> Allow
    Verdict --> Warn
    Verdict --> Block
    Telemetry --> Audit
    Audit --> Config
```

## What this shows

- **Policy lives with the code.** The `.hector.yml` travels with the repo, so every agent sees the same rules and severities.
- **Adapters are thin.** Claude Code, OpenCode, Reasonix, pi, and future adapters capture host events and consume Hector's verdict. Policy logic stays in `hector-core`.
- **Two engines, one gate.** Shell checks cover anything a command can decide; AST matching catches code structure a regex would miss. Both are deterministic and run locally.
- **Trust comes before power.** Script rules can execute commands, so Hector verifies the signed config before any rule runs.
- **The verdict is machine-readable.** `pass`, `warn`, `block`, and `internal_error` map to stable exit codes that agents and CI can act on automatically. Per-edit gates block immediately so the agent retries before the change lands.
- **The system improves over time.** Baselines and disables keep adoption practical; telemetry shows which rules are noisy, valuable, or dead.

## Mental model

Hector is not another linter. It is the policy layer around AI-generated edits: local enough to understand a repository's rules, and structured enough to turn them into deterministic gates an agent must clear before its edit lands.
