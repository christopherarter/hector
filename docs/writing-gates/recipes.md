# Gate recipes

Worked gates for policies you'll actually want. Each is a complete `.hector.yml` entry; drop it under `gates:` and adjust the id, glob, and command. For the rules these rely on — the exit-code contract and the ABI — see [Anatomy of a gate](README.md).

## Ban a pattern with grep

The simplest gate: block an edit when a forbidden string appears.

```yaml
gates:
  no-focused-tests:
    files: "**/*.test.ts"
    run: "! grep -nH '\\.only(' \"$HECTOR_FILE\" || exit 2"
```

`grep` exits `0` on a match, so `! grep …` flips that to a failure on a hit, and `|| exit 2` turns the failure into a block. `-nH` makes grep print `file:line:match`, which becomes the message the agent sees.

## Run a linter over the proposed content

A linter that reads stdin can check the new bytes before they land. Move it into a script so it stays readable:

```yaml
gates:
  biome:
    files: ["src/**/*.ts", "src/**/*.tsx"]
    run: ".hector/gates/biome.sh"
```

```sh
# .hector/gates/biome.sh
#!/usr/bin/env sh
# Lint the proposed content, which Hector delivers on stdin.
biome check --stdin-file-path "$HECTOR_FILE" || exit 2
```

`biome check` exits non-zero when it finds problems; `|| exit 2` promotes that to a block. Because it reads stdin, it sees the edit the agent is *proposing*, not whatever is currently on disk. Make the script executable: `chmod +x .hector/gates/biome.sh`.

## Run a whole-tree tool

Some checks need a real, consistent file tree — a dependency-graph rule, a typechecker that resolves imports. These ignore stdin and read the on-disk tree from `$HECTOR_ROOT`:

```yaml
gates:
  depcruise:
    files: "src/**/*.ts"
    run: "npx depcruise --validate .dependency-cruiser.js src || exit 2"
```

The gate's working directory is `$HECTOR_ROOT`, so a relative path like `src` resolves against the project root. In a batch run this re-runs once per changed file, which is redundant but correct — the check is idempotent.

## Ask a model to judge

Because a gate is just a command that exits `2`, a model can be the judge:

```yaml
gates:
  no-secrets:
    files: "**/*"
    run: ".hector/gates/secret-scan.sh"
```

```sh
# .hector/gates/secret-scan.sh
#!/usr/bin/env sh
content=$(cat)
verdict=$(printf '%s' "$content" | claude -p "Reply BLOCK if this file contains a hardcoded secret, otherwise PASS.")
case "$verdict" in
  *BLOCK*) echo "Possible hardcoded secret — move it to an environment variable." >&2; exit 2 ;;
  *)       exit 0 ;;
esac
```

`cat` reads the proposed content from stdin. The gate decides on its own judgement and exits `2` to block, with no special support from Hector.

## Block only on a specific event

`$HECTOR_EVENT` tells a gate how it was triggered, so you can be strict at commit time but lenient on live edits:

```yaml
gates:
  tests-pass-precommit:
    files: "src/**/*.rs"
    run: "[ \"$HECTOR_EVENT\" = pre-commit ] || exit 0; cargo test -q || exit 2"
```

On any event other than `pre-commit`, the gate exits `0` immediately. At commit time it runs the tests and blocks if they fail.

## See also

- [Anatomy of a gate](README.md) — the contract these recipes rely on
- [Targeting files](../configuring/targeting-files.md) — the `files:` globs
- [The trust store](../security/trust.md) — why editing a gate script re-triggers a blessing
