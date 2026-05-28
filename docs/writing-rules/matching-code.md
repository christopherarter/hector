# Matching code structure

The `ast` engine matches code by its syntax tree, not its text. Use it to ban a call, macro, or construct regardless of whitespace, line breaks, or variable names — the things a regex gets wrong.

```yaml
rules:
  no-unwrap-in-src:
    description: "Avoid .unwrap() in production source. Use ? or expect() with a message."
    engine: ast
    language: rust
    scope: ["crates/*/src/**/*.rs"]
    severity: warning
    pattern: "$EXPR.unwrap()"
```

This matches every `.unwrap()` call in scope — `foo.unwrap()`, `bar().baz.unwrap()`, `self.cache.get(k).unwrap()` — because `$EXPR` is a metavariable that stands for any expression. Each match becomes its own violation, with the line, column, and a few lines of surrounding context.

## `pattern` and `language`

Two fields beyond the required four:

- **`pattern`** — an [ast-grep](https://ast-grep.github.io/guide/pattern-syntax.html) pattern. Write a snippet of the code you want to catch, with metavariables for the parts that vary.
- **`language`** — the language to parse the file as. Required; Hector does not infer it. Use the ast-grep language name: `rust`, `ts`, `tsx`, `js`, `python`, `go`, `java`, and so on.

## Metavariables

Metavariables are how a pattern generalizes over the parts you don't care about:

- **`$NAME`** matches a single node — one expression, one identifier, one argument.
- **`$$$NAME`** (or bare `$$$`) matches zero or more nodes — a whole argument list, a sequence of statements.

```yaml
rules:
  no-todo-in-src:
    description: "todo!() must not ship. Replace with a real implementation or return an error."
    engine: ast
    language: rust
    scope: ["crates/*/src/**/*.rs"]
    severity: error
    pattern: "todo!($$$)"
```

`todo!($$$)` matches `todo!()`, `todo!("not yet")`, and `todo!("blocked on {}", id)` alike — `$$$` absorbs whatever arguments are there.

## Why this beats a regex

A regex over source text breaks on the cases that matter:

```rust
let x = foo            // a regex for `.unwrap()` on one line misses this
    .unwrap();
let s = "call .unwrap() in the docs";  // and false-positives on this
```

The `ast` pattern catches the first (it's a real call across two lines) and ignores the second (it's a string literal, not a call). Structure, not text.

## One violation per match

Where `script` and `semantic` rules report at most one finding per file, `ast` reports every node that matches. A file with five `.unwrap()` calls produces five violations, each pointing at its own line and column. That makes AST rules the most precise for editor and CI annotations.

## Common patterns

```yaml
rules:
  no-as-any:
    description: "No `as any` casts."
    engine: ast
    language: ts
    scope: ["src/**/*.ts"]
    severity: error
    pattern: "$EXPR as any"

  no-panic-in-src:
    description: "Avoid panic!() in production source. Return an error instead."
    engine: ast
    language: rust
    scope: ["crates/*/src/**/*.rs"]
    severity: warning
    pattern: "panic!($$$)"

  no-print-in-src:
    description: "No print() in library code — use logging."
    engine: ast
    language: python
    scope: ["src/**/*.py"]
    severity: warning
    pattern: "print($$$)"
```

To check a pattern interactively before committing it to a rule, use the [ast-grep playground](https://ast-grep.github.io/playground.html) — the pattern syntax is identical.

## See also

- [Running a shell check](shell-checks.md) — when text matching is enough
- [ast-grep pattern syntax](https://ast-grep.github.io/guide/pattern-syntax.html) — the full pattern language
- [Config schema](../reference/config-schema.md#rule) — every `ast` rule field
