use crate::spec::{Arch, Target, base};

pub(crate) fn target() -> Target {
    // Above the kernel image (2..34 MiB) and the boot identity map around it:
    // the user window is a band that starts here (`PHYSMAP.md` P4). The loader
    // sits one step above the shared objects, which `ldso/src/layout.rs`
    // `DSO_BASE`/`DSO_LIMIT` echo.
    let mut base = base::minix::opts(&[
        "--image-base=0x4000000",
        "--defsym=BASE_ADDRESS=0x4000000",
        "--defsym=LOADER_BASE=0x8000000",
    ]);
    base.cpu = "x86-64".into();
    base.max_atomic_width = Some(64);

    Target {
        llvm_target: "x86_64-unknown-none".into(),
        metadata: crate::spec::TargetMetadata {
            description: Some("Minix (x86_64)".into()),
            tier: Some(3),
            host_tools: Some(false),
            std: Some(true),
        },
        pointer_width: 64,
        data_layout:
            "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128".into(),
        arch: Arch::X86_64,
        options: base,
    }
}
