# Disabling a gate in-line

To turn one gate off for a single file, put a `hector-disable:` directive anywhere in that file — usually in a comment — naming the gate id:

```rust
// hector-disable: no-unwrap-in-src
let port = env::var("PORT").unwrap();
```

The directive suppresses the named gate **for that whole file**. It is always file-wide — there is no per-line scope. A gate produces one verdict per file, so a directive anywhere in the content disables it.

## Writing the directive

A directive names a gate id. The id runs up to whitespace, a comma, a `*`, or a `/` that begins a comment terminator (`//` or `/*`), so the directive sits cleanly inside comments in any language:

```python
# hector-disable: no-print
print(debug_state)
```

```javascript
/* hector-disable: no-console */
console.log(state);
```

A `/` only ends the id when it begins `//` or `/*`, so namespaced ids like `python/no-print` survive intact.

To turn off several gates at once, separate the ids with spaces or commas:

```python
x = eval(s)  # hector-disable: no-eval, py/no-dynamic
```

## See also

- [Targeting files](targeting-files.md) — the `files:` globs a gate matches
- [Config schema](../reference/config-schema.md) — the full gate shape
