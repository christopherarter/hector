# Hector — Capability Enforcement

## Status by platform

| Platform | `network: false` | `writes: none` / `cwd-only` |
|----------|------------------|------------------------------|
| Linux    | Strict (CLONE_NEWNET namespace) | Best-effort (requires user-namespace privilege; degrades gracefully) |
| macOS    | Best-effort (advisory, logged) | Best-effort (advisory, logged) |
| Windows  | Not supported in 0.1 | Not supported in 0.1 |

## Threat model

Capabilities protect against accidental damage from misconfigured `script:` rules, not against adversarial rule authors. The `trust` gate is the primary defense: rules cannot run until the user reviews and trusts the config.

## Writes policy enforcement (0.1)

The schema accepts `writes: none | cwd_only | tmp | unrestricted` but
**0.1 does not enforce any of them**. All four behave identically:
the spawned process can write anywhere it has POSIX permission to.

Why: enforcement requires CAP_SYS_ADMIN inside a user namespace plus
careful bind-mount remounts; the work is tracked for 0.2. Until then,
treat `writes:` as advisory documentation, not as a control.

If you need write isolation today, run hector inside an OS-level
sandbox (e.g., a container, a fresh user, or `bwrap`).

## Roadmap

macOS sandbox profile integration is tracked in `specs/2026-05-11-hector-plan-and-0.1-design.md` §13 (risks). Re-evaluated at 1.0.
