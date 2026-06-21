# qr-gen

A fast, fully-offline command-line QR code generator written in Rust. Encode
text, URLs, or vCards into **PNG** or **SVG**, one at a time or in **batch**.
No network access is ever performed.

## Features

- **PNG and SVG output** — pick with `--svg`, or let the `-o` extension decide.
- **Single or batch** — encode one string, or a whole `.txt`/`.csv` file at once.
- **Error-correction control** — `--ecc l|m|q|h` (default `m`).
- **Custom colors** — `--fg` / `--bg` accept `#rgb`, `#rgba`, `#rrggbb`,
  `#rrggbbaa`, or the names `black` / `white`.
- **Scalable** — `--size` sets the minimum image edge in pixels;
  `--quiet-zone` sets the surrounding margin in modules.
- **vCard convenience** — build a valid vCard 3.0 from `key=value` pairs.
- **Verified correctness** — the test suite generates QR PNGs and *decodes them
  back* with an independent decoder (`rqrr`), asserting the payload survives the
  round trip.

## Install / Build

```sh
cargo build --release
# binary at target/release/qr-gen
```

## Usage

```
qr-gen [PAYLOAD] [OPTIONS]
qr-gen --batch <FILE> [--outdir <DIR>] [OPTIONS]
qr-gen --vcard <KEY=VALUE>... [OPTIONS]
```

### Options

| Flag | Description | Default |
| --- | --- | --- |
| `[PAYLOAD]` | Text or URL to encode (single mode) | — |
| `-o, --output <PATH>` | Output file; format inferred from extension | `qr.png` |
| `--svg` | Emit SVG instead of PNG (overrides extension) | off |
| `--size <PX>` | Minimum image edge length in pixels | `512` |
| `--ecc <L\|M\|Q\|H>` | Error-correction level | `m` |
| `--fg <HEX>` | Foreground/module color | `#000000` |
| `--bg <HEX>` | Background color | `#ffffff` |
| `--quiet-zone <MODULES>` | Margin width in modules (`0` disables) | `4` |
| `--batch <FILE>` | Batch input: `.txt` (one line per QR) or `.csv` | — |
| `--outdir <DIR>` | Output directory for batch mode (auto-created) | `qrs` |
| `--vcard <KEY=VALUE>...` | Build a vCard from fields (see below) | — |

### Examples

Encode a URL to a PNG:

```sh
qr-gen "https://example.com" -o out.png
```

Encode to SVG with high error correction and custom colors:

```sh
qr-gen "https://example.com" -o out.svg --ecc h --fg '#1a1a2e' --bg '#eeeeee'
```

Larger image, no quiet zone:

```sh
qr-gen "hello world" -o big.png --size 1024 --quiet-zone 0
```

### Batch mode

**Plain text** — one payload per non-empty line (lines starting with `#` are
treated as comments and skipped). Files are named with a zero-padded index plus
a slug of the payload:

```sh
qr-gen --batch lines.txt --outdir ./qrs
# ./qrs/0001-https-example-com.png, ./qrs/0002-hello-world.png, ...
```

**CSV** — a `text` column (or the first column if no recognizable header)
supplies the payload; an optional `filename` column sets the output stem:

```csv
text,filename
https://one.example,alpha
https://two.example,beta
```

```sh
qr-gen --batch data.csv --outdir ./qrs        # -> alpha.png, beta.png
qr-gen --batch data.csv --outdir ./qrs --svg  # -> alpha.svg, beta.svg
```

Recognized payload columns: `text`, `payload`, `data`, `url`.
Recognized filename columns: `filename`, `file`, `name`.
Filenames are sanitized (no path traversal, extension stripped).

### vCard mode

Build a valid vCard 3.0 and encode it. `name` is required; all other fields are
optional:

```sh
qr-gen --vcard name="Jane Doe" email=jane@x.com phone=+15551234 -o jane.png
```

Supported fields: `name`, `email`, `phone`, `org`, `title`, `url`, `address`,
`note`. Scanning the resulting QR adds the contact on most phones.

## Exit codes

`qr-gen` exits non-zero with a clear message on stderr for invalid input —
e.g. a malformed hex color, an unreadable batch file, an oversized payload, or
no input at all.

## Testing

```sh
cargo test
```

The suite includes the strong round-trip check: a PNG is generated for a URL, a
long string, and a vCard, then decoded with `rqrr` and asserted byte-equal to
the input. CLI behavior, SVG markup, batch file counts, and error/exit-code
paths are all covered.

## License

MIT — see [LICENSE](LICENSE).
