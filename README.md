# oxi-gen

A high-performance command-line tool for converting CSV/TSV files to RDF (Turtle or N-Triples) using SPARQL CONSTRUCT queries. Inspired by [Tarql](https://tarql.github.io/), oxi-gen is built in Rust on top of the [Oxigraph](https://github.com/oxigraph/oxigraph) stack and leverages multi-threaded processing to handle large datasets efficiently.

Each row of the input CSV is bound as SPARQL variable substitutions and evaluated against a CONSTRUCT query, producing RDF output. Column headers become variable names (e.g., a `name` column is available as `?name`), and the special variable `?ROWNUM` holds the current row index.

## Command-Line Options

```
oxi_gen -q <QUERY> [OPTIONS]
```

| Option | Short | Description |
|---|---|---|
| `--query <FILE>` | `-q` | SPARQL CONSTRUCT query file to apply (required) |
| `--input <FILE>` | `-i` | Input CSV file. Omit to read from STDIN |
| `--output <FILE>` | `-o` | Output file. Omit to write to STDOUT |
| `--delimiter <CHAR>` | `-d` | CSV delimiter character (default: `,`) |
| `--tab` | `-t` | Treat input as tab-separated (TSV) |
| `--no-header-row` | `-H` | Input has no header row; columns are named `a`–`z`, `A`–`Z` |
| `--normalize` | `-n` | Normalize column names to UPPERCASE |
| `--escape_char <CHAR>` | `-p` | Escape character (default: `\`) |
| `--quote_char <CHAR>` | | Quote character (default: `"`) |
| `--ntriples` | | Output N-Triples instead of Turtle |
| `--gzip` | `-g` | Gzip the output (requires `--output`) |
| `--dedup[=N]` | | Deduplicate triples within a sliding window (default window: 1000, range: 1000–5000000) |
| `--test[=N]` | | Process only the first N rows for testing (default: 5, max: 49) |
| `--split <ORIGINAL> <SPLIT> <DELIMITER>` | | Split column ORIGINAL on DELIMITER, binding each value to SPLIT. Can be repeated |
| `--bind-empty-strings` | | Bind empty CSV values as empty string literals instead of skipping them |

## Custom SPARQL Functions

oxi-gen registers two custom functions under the `tarql:` prefix (`https://semanticarts.com/tarql/`):

- **`tarql:expandPrefix(?prefix)`** — returns the IRI for a given prefix name declared in the query.
- **`tarql:expandPrefixedName(?qname)`** — expands a prefixed name (e.g., `"foaf:name"`) into a full IRI node.

## Building from Source

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (1.85+ required for edition 2024)

### Build a release binary

```sh
git clone git@github.com:semanticarts/oxi-gen.git
cd oxi-gen
cargo build --release
```

The optimized binary will be at `target/release/oxi_gen`. The release profile is configured with LTO, single codegen unit, and abort-on-panic for maximum performance.

### Run directly with Cargo

```sh
cargo run --release -- -q query.sparql -i data.csv -o output.ttl
```
