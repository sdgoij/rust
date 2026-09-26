use crate::spec::{Arch, Target, base};

pub(crate) fn target() -> Target {
    let mut base = base::minix::dyn_opts();
    base.features = "+v8a,+strict-align,+neon".into();
    base.max_atomic_width = Some(128);

    Target {
        llvm_target: "aarch64-unknown-none".into(),
        metadata: crate::spec::TargetMetadata {
            description: Some("Minix (aarch64), position-independent: shared objects and PIC code"
                .into()),
            tier: Some(3),
            host_tools: Some(false),
            // Only `core` and `alloc` are built for this target — a shared object
            // links nothing else — which is what `no-std` in the OS's generated
            // `config.toml` says, matching the `*-none-*` targets.
            std: Some(false),
        },
        pointer_width: 64,
        data_layout:
            "e-m:e-p270:32:32-p271:32:32-p272:64:64-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128-Fn32"
                .into(),
        arch: Arch::AArch64,
        options: base,
    }
}
