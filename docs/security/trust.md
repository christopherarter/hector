# The trust gate

A `.hector.yml` can run arbitrary shell commands through its `script:` rules. So Hector refuses to run a config it hasn't verified you've seen. You review the config, sign it once, and Hector checks that signature before every run. This is the primary defense against a malicious or tampered config.

## Signing a config

After writing or editing your rules, review them, then sign:

```bash
hector trust
```

This computes a SHA-256 fingerprint of the config and writes it into the `trust:` block:

```yaml
trust:
  fingerprint: sha256:8798ad5a0ab624c9a5d56b87372cdaf1fdd3ccc5339fe2573b82b26be28b9f36
```

To sign a config other than `.hector.yml`:

```bash
hector trust --config shared/base.yml
```

## How verification works

Before running any rule, `hector check` recomputes the fingerprint and compares it to the one in the `trust:` block. If they don't match, the check stops with a config error (exit `1`) and a hint to re-sign — no rule runs.

Any change to the rules invalidates the fingerprint. That's the point: a config that's been edited (by you, a teammate, or anything else) since you last reviewed it won't run until you look at the change and re-sign.

## Re-signing after an edit

Every time you change the config, re-run `hector trust`. The workflow is: edit rules → review → `hector trust` → commit. If you pull a change to `.hector.yml` from a teammate, review their diff before signing it on your machine.

Upgrading Hector across a version that changed the fingerprint algorithm also requires a one-time re-sign; the mismatch error from `hector check` will say so.

## The fingerprint algorithm

The fingerprint is computed over a canonical form of the config so that cosmetic differences — key order, indentation, quoting style — don't change it:

1. Parse the YAML and strip the `trust:` block (the fingerprint can't include itself).
2. Convert to JSON and sort all keys recursively.
3. Serialize to a JSON string and take its SHA-256.

JSON's byte form is specified by RFC 8259, so the hash is stable across library version bumps. A config that uses YAML anchors/aliases (`&name`/`*name`), non-string mapping keys, or non-finite numbers can't be canonicalized and produces an error rather than a silent hash — write the config out in plain form to sign it.

## What trust does and doesn't cover

The trust gate is the defense against **malicious** rules: nothing runs until you've reviewed and signed it. The [capability sandbox](capabilities.md) is a separate, weaker layer that limits the damage from a *misconfigured* (not malicious) script — and it isn't a security boundary against an attacker.

If you need real isolation from a config you don't fully trust, run Hector inside an OS-level sandbox — a container, a fresh user, or `bwrap` — in addition to the trust gate.

Trust is also **never inherited**. When you use `extends:`, every config file that contains rules carries its own signed fingerprint, so a parent change can't slip new rules under a child's existing signature. See [Sharing config with `extends:`](../configuring/inheritance.md).

## See also

- [Capability sandboxing](capabilities.md) — the network and write constraints for `script:` rules
- [Sharing config with `extends:`](../configuring/inheritance.md) — why each file signs itself
- [Getting started](../getting-started.md) — trust in the first-run workflow
