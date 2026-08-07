use crate::spec::{Arch, LlvmAbi, Target, base};

pub(crate) fn target() -> Target {
    let mut base = base::minix::opts();
    base.cpu = "generic-rv64".into();
    base.features = "+m,+a,+f,+d,+c".into();
    base.max_atomic_width = Some(64);
    base.llvm_abiname = LlvmAbi::Lp64d;

    Target {
        llvm_target: "riscv64-unknown-none-elf".into(),
        metadata: crate::spec::TargetMetadata {
            description: Some("Minix (riscv64gc)".into()),
            tier: Some(3),
            host_tools: Some(false),
            std: Some(true),
        },
        pointer_width: 64,
        // Must match the LLVM default for `riscv64-unknown-none-elf` (the
        // `n32:64` native vector sizes), or rustc rejects the target.
        data_layout: "e-m:e-p:64:64-i64:64-i128:128-n32:64-S128".into(),
        arch: Arch::RiscV64,
        options: base,
    }
}
