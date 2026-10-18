# ESC/POS golden fixtures

Each `NN-name.bin` is a raw byte stream as an app would send it to a receipt printer. The
expected results are snapshots in `snapshots/`:

- `NN-name.ast.snap`: the parsed AST (`hardless-escpos`).
- `NN-name.html.snap` and `NN-name.text.snap`: the rendered receipt (`hardless-render`).

The tests also check that every fixture parses to the same AST however its bytes are split
into chunks, including at every single split point.

## Changing fixtures

The `.bin` files are generated from readable definitions in
[`scripts/gen-escpos-fixtures.mjs`](../../scripts/gen-escpos-fixtures.mjs). After adding or
changing one:

```sh
node scripts/gen-escpos-fixtures.mjs
INSTA_UPDATE=always cargo test -p hardless-escpos -p hardless-render
git diff fixtures/escpos/snapshots   # review every change before committing
```

Byte sequences follow the publicly available ESC/POS command reference.
