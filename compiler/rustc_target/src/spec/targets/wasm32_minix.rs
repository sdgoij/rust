//! WebAssembly with the MINIX ABI: the target the port's wasm kernel, servers
//! and program modules are built for, so they compile with the real
//! `target_os = "minix"` bodies rather than a host stub.
//!
//! Everything but the OS, the entry-point flags and the metadata is
//! `base::wasm::options()`: `only_cdylib` (a module is a cdylib), `pic`-free
//! `static` relocations, `local-exec` TLS, the 1 MiB stack-first pre-link args
//! and `-wasm-use-legacy-eh=false`. The host owns each instance's memory, so the
//! memory flags live in the crates' own `.cargo/config.toml` (`--import-memory`,
//! `--initial-memory`), not here.

use crate::spec::{Arch, Cc, LinkerFlavor, Os, Target, TargetMetadata, base};

pub(crate) fn target() -> Target {
    let mut options = base::wasm::options();
    options.os = Os::Minix;

    options.add_pre_link_args(
        LinkerFlavor::WasmLld(Cc::No),
        &[
            // The module's entry is an exported function the host calls, not
            // `_start`, so there is no entry symbol to look for.
            "--no-entry",
        ],
    );
    options.add_pre_link_args(
        LinkerFlavor::WasmLld(Cc::Yes),
        &[
            // Make sure clang uses LLD as its linker and is configured
            // appropriately otherwise.
            "--target=wasm32-unknown-unknown",
            "-Wl,--no-entry",
        ],
    );

    Target {
        llvm_target: "wasm32-unknown-unknown".into(),
        metadata: TargetMetadata {
            description: Some("WebAssembly (MINIX ABI)".into()),
            tier: Some(3),
            host_tools: Some(false),
            // Only `core` and `alloc` are built for this target — a module links
            // nothing else — which is what `no-std` in the OS's generated
            // `config.toml` says.
            std: Some(false),
        },
        pointer_width: 32,
        data_layout: "e-m:e-p:32:32-p10:8:8-p20:8:8-i64:64-i128:128-n32:64-S128-ni:1:10:20".into(),
        arch: Arch::Wasm32,
        options,
    }
}
