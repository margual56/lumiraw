# rawler 0.8.0, vendored

The raw decoder, from dnglab (<https://github.com/dnglab/dnglab>), LGPL-2.1 as
the `LICENSE` beside this file says. It replaced `rawloader`, whose camera
list stopped around 2020 and which has no CR3 decoder at all.

It is vendored rather than taken from crates.io for one reason: it depends on
`chrono` with default features, and one of those, `wasmbind`, makes the module
import functions from wasm-bindgen's JavaScript glue. This module is loaded
without wasm-bindgen, so those imports cannot be satisfied and the module
would not instantiate. The only change is in `Cargo.toml`:

- `chrono` is taken with `default-features = false, features = ["std", "clock"]`.
- The binaries, tests and benchmarks are dropped, along with the 39 MB of
  sample files they read (`data/testdata`).

Everything under `src/` and `data/cameras` is as published. To update, copy a
newer release over this directory and repeat the two edits above.
