# yaml-flatten

Nested YAML config is hard to `grep`, hard to `diff` line-by-line, and awkward
to feed into shell scripts that just want `KEY=value` pairs. `yaml-flatten`
reads a YAML file and prints every leaf value as a dotted path, one per line.

## Usage

```
yaml-flatten <file.yaml|->
```

Pass `-` to read from stdin.

## Example

Given `config.yaml`:

```yaml
service:
  name: payments-api
  port: 8080
  debug: false
database:
  host: db.internal
  replicas:
    - db-replica-1
    - db-replica-2
tags:
  - name: env
    value: production
  - name: team
    value: payments
```

Running:

```
$ yaml-flatten config.yaml
service.name=payments-api
service.port=8080
service.debug=false
database.host=db.internal
database.replicas[0]=db-replica-1
database.replicas[1]=db-replica-2
tags[0].name=env
tags[0].value=production
tags[1].name=team
tags[1].value=payments
```

That output is diffable, greppable, and easy to turn into environment
variables with a small amount of shell (`sed 's/\./_/g'`, uppercase, export).

## Building

```
cargo build --release
```

The binary is at `target/release/yaml-flatten`. No third-party crates - the
whole thing is standard library.

## What it supports

Block-style YAML: nested mappings, sequences (including sequences of
mappings), scalars, comments, and blank lines. Strings, numbers, booleans
(`true`/`false`), and null (`null`/`~`/empty) are recognized.

## What it doesn't (yet)

- Flow style (`{a: 1}`, `[1, 2]`)
- Anchors and aliases (`&foo`, `*foo`)
- Multi-line block scalars (`|`, `>`)
- Multiple documents in one file (`---` is skipped, not treated as a
  separator)

If a file uses any of these, parsing will either fail with a line number or
produce output that doesn't match the source - check the flattened output
against the input for anything unusual.

## Design note

Every function that does real work (`parser::parse`, `parser::parse_scalar`,
`flatten::flatten`) is a pure function: string or value in, value or string
out, no file or stdin access. All I/O lives in `main.rs`. That split is what
makes the parser's test suite run without touching a filesystem.
