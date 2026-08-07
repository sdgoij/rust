use crate::spec::{Arch, CodeModel, LlvmAbi, Target, base};

pub(crate) fn target() -> Target {
    let mut base = base::minix::opts();
    base.cpu = "generic-rv64".into();
    base.features = "+m,+a,+f,+d,+c".into();
    base.max_atomic_width = Some(64);
    base.llvm_abiname = LlvmAbi::Lp64d;
    // medany (PC-relative): the kernel image's `.bss` sits >512 KB past the
    // start of `.text` (the embedded initramfs pushes the layout), and the
    // default medlow model's absolute `lui` references fail lld's HI20 range
    // check. Matches the upstream `riscv64gc-unknown-none-elf` spec.
    base.code_model = Some(CodeModel::Medium);

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
