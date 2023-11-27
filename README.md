# ya-runtime-ai

## Provider setup

Run `cargo build --workspace`.
Create exeunits descriptor json file using [ya-runtime-ai.json](conf/ya-dummy-ai.json) as an example (with correct `supervisor-path`).

Point `ya-provider` to exeunits descriptor using `EXE_UNIT_PATH` variable.

`ya-provider` creates on startup a `default` preset for `wasmtime` runtime.
Update it: `ya-provider.exe preset update --name default  --no-interactive  --exe-unit ai --price Duration=0.0001 CPU=0.0001 "Init price=0.0000000000000001"`
