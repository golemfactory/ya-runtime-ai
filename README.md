# ya-runtime-ai

## Provider setup

Run `cargo build --workspace`.
Create sample `exeunits-descriptor.json` (with correct `supervisor-path`):

```json
[
    {
        "name": "ai-dummy",
        "version": "0.1.0",
        "supervisor-path": "[..]\\ya-runtime-ai\\target\\debug\\ya-runtime-ai.exe",
        "extra-args": ["--runtime", "dummy"],
        "description": "dummy ai runtime",
        "properties": {
        }
    }
]
```

Point `ya-provider` to exeunits descriptor using `EXE_UNIT_PATH` variable.

`ya-provider` creates on startup a `default` preset for `wasmtime` runtime.
Update it: `ya-provider.exe preset update --name default  --no-interactive  --exe-unit ai-dummy --price Duration=1.2 CPU=3.4 "Init price=0.00001"`
