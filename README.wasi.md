# to build wasi embeddable module

cargo rustc --lib --crate-type cdylib --release --target wasm32-wasip1 --no-default-features --features "feat_wasm feat_wasm_reactor"
