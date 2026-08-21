# regex-strings demo

This is a small static frontend for the `regex-strings` crate. The Rust adapter
in `src/lib.rs` is a separate package so the published crate stays focused on
its library API.

## Local preview

From the repository root:

```sh
wasm-pack build web --target web --release --out-dir pkg --no-typescript
python3 -m http.server 4173 --directory web
```

Then open <http://127.0.0.1:4173>.

The GitHub Actions workflow builds `web/` and deploys the resulting static
folder to GitHub Pages whenever `master` changes.
