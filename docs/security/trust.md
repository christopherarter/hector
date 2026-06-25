# The trust gate

A `.hector.yml` runs arbitrary shell through its gates' `run:` commands. So Hector refuses to run a config you haven't vouched for: you review the config and its gate scripts, bless them once, and Hector verifies that blessing before every `check`. This is the primary defense against a malicious or tampered config.

The blessing lives **outside the repo**, so a config can't vouch for itself — a pulled, generated, or freshly-edited config is untrusted until *you*, on *this* machine, bless it.

## Blessing a config

After writing or editing your gates, review them, then bless:

```bash
hector trust
```

This computes a SHA-256 over the config and its gate scripts and records it in the trust store at `~/.config/hector/trust.json` (or `$XDG_CONFIG_HOME/hector/trust.json`), keyed by the config's absolute path:

```json
{
  "version": 1,
  "entries": {
    "/home/you/project/.hector.yml": {
      "hash": "sha256:8798ad5a0ab624c9a5d56b87372cdaf1fdd3ccc5339fe2573b82b26be28b9f36",
      "blessed_at": "2026-06-24T18:03:11+00:00"
    }
  }
}
```

To bless a config other than `.hector.yml`:

```bash
hector trust --config shared/base.yml
```

## How verification works

Before loading the engine or running any gate, `hector check` recomputes the hash and compares it to the blessed entry for that config's path. On a missing or mismatched entry it stops with a config error (exit `1`) and a hint to re-bless — no gate runs:

```
config/gates not trusted — review and run `hector trust`
```

Only `check` enforces trust. The read-only commands — `validate`, `explain`, `show-resolved-config`, `doctor` — never do, so you can inspect an untrusted config without blessing it first.

Any change to a covered file invalidates the hash. That's the point: a config, or a gate script, that's been edited — by you, a teammate, or anything else — since you last reviewed it won't run until you look at the change and re-bless.

## Re-blessing after a change

Re-run `hector trust` whenever you change anything it covers (see [What trust covers](#what-trust-covers)):

- the config file, or any file it `extends:`
- any script under a covered `.hector/gates/`

The workflow is: edit gates → review → `hector trust` → commit. If you pull a change to `.hector.yml` (or a base it extends) from a teammate, review their diff before blessing it on your machine.

The trust store lives outside the repo, so it isn't committed and isn't shared — every machine blesses for itself. Moving the project to a new path, or upgrading Hector across a version that changes the hash algorithm, also needs a one-time re-bless; the mismatch error from `check` will say so.

## What trust covers

The blessed hash folds, in a fixed, deterministic order:

1. **Every config file in the `extends:` closure** — the config you check, plus every file it transitively extends.
2. **Every file under each of their `.hector/gates/` directories.**

So with `extends:` you bless the **root** config you run `check` against, and a single `hector trust` covers the whole chain. Editing a parent — or a parent's gate script — invalidates the root's hash and forces a re-review. (You only bless a parent separately if you also `check` it directly as a root of its own.)

### What it doesn't cover

- **Gate scripts outside `.hector/gates/`.** A gate whose `run:` shells out to a file elsewhere in the repo — `run: "bash scripts/lint.sh"` or `run: "python tools/scan.py"` — is covered only for the `run:` *string* (which lives in the config). The contents of `scripts/lint.sh` are **not** hashed, so editing that file can neuter the gate without invalidating trust. Keep gate logic under `.hector/gates/` (e.g. `run: ".hector/gates/lint.sh"`) to bring it inside the boundary. This is the same threat class as a tampered config, reached through a file the hash doesn't reach.
- **Interpreters and tools on `$PATH`.** Trust vouches for your gate scripts, not for the `python`, `grep`, or `node` they invoke.
- **Writes during the run.** The hash is computed when `check` starts; a write between that point and a gate actually executing isn't caught. This TOCTOU window is a known limitation of the direnv-style model — there's no file locking in 0.3.

## Trust is not a sandbox

The trust gate answers "have I reviewed what runs?", not "what can it do once it runs?". Hector 0.3 does **not** sandbox gate commands — the per-gate [timeout](../operating/running-checks.md) is the only execution rail, and a blessed gate runs with your full user privileges.

If you need real isolation from a config you don't fully trust, run Hector inside an OS-level sandbox — a container, a fresh user, or `bwrap` — in addition to blessing it.

## See also

- [Sharing config with `extends:`](../configuring/inheritance.md) — blessing a config that inherits
- [Running checks](../operating/running-checks.md) — where trust sits in the `check` flow
- [Getting started](../getting-started.md) — trust in the first-run workflow
