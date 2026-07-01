# Disabling a check in-line

To turn one check off for a single file, put a `ironlint-disable:` directive anywhere in that file — usually in a comment — naming the check id:

```rust
// ironlint-disable: no-unwrap-in-src
let port = env::var("PORT").unwrap();
```

The directive suppresses the named check **for that whole file**. It is always file-wide — there is no per-line scope. A check produces one verdict per file, so a directive anywhere in the content disables it.

## Writing the directive

A directive names a check id. The id runs up to whitespace, a comma, a `*`, or a `/` that begins a comment terminator (`//` or `/*`), so the directive sits cleanly inside comments in any language:

```python
# ironlint-disable: no-print
print(debug_state)
```

```javascript
/* ironlint-disable: no-console */
console.log(state);
```

A `/` only ends the id when it begins `//` or `/*`, so namespaced ids like `python/no-print` survive intact.

To turn off several checks at once, separate the ids with spaces or commas:

```python
x = eval(s)  # ironlint-disable: no-eval, py/no-dynamic
```

## See also

- [Targeting files](targeting-files.md) — the `files:` globs a check matches
- [Config schema](../reference/config-schema.md) — the full check shape
