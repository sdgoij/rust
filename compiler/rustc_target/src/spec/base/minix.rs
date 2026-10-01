//! The `*-minix` base target.
//!
//! Two forms are exported: [`opts`] for executables, which takes this arch's
//! userland image base, and [`dyn_opts`] for the position-independent form a
//! loader maps as a shared object.

use crate::spec::{Cc, LinkerFlavor, Lld, Os, PanicStrategy, RelocModel, TargetOptions};

/// The executable form of the target.
///
/// `pre_link_args` carries this arch's userland base: the `--image-base=` flag,
/// which pins where an *unlinked* binary's headers go (lld's default, 0x200000, is
/// inside the kernel image on x86_64, so such a binary would silently alias kernel
/// memory at run time), and the `--defsym` assignments that
/// `tools/minix-user.ld` / `tools/minix-ldso.ld` take their `PROVIDE`d bases from.
///
/// It has to be `--image-base` rather than `-Ttext` because the kernel's exec
/// loader derives the code span from the lowest `PT_LOAD`, so the headers segment
/// must land at the base too.
///
/// The `--defsym` is the half that actually moves the sections: with a `-T` script
/// an explicit section address wins, and lld evaluates the script's `PROVIDE`
/// before the trailing `-C link-arg` flags — so these come from here, where rustc
/// places them ahead of every `-T` the build passes. That is also why the base is
/// set here rather than in each build script: the spec is the only per-arch channel
/// every link of this target goes through (the Justfile, `tools/build-*.py`,
/// `tools/cc-minix.py`, `tools/mkinitramfs.rs`).
///
/// The base is per-arch because it has to clear the kernel image and the identity
/// map the kernel runs on (`PHYSMAP.md` P4): 16 MiB on riscv64 and aarch64, whose
/// images live at or above the RAM base, and 64 MiB on x86_64, whose image is
/// loaded at 2 MiB and reaches 34 MiB. `LOADER_BASE` is the loader's own base, one
/// step above the shared-object region, which is why it is a second symbol.
pub(crate) fn opts(pre_link_args: &'static [&'static str]) -> TargetOptions {
    minix_opts(false, pre_link_args)
}

/// The position-independent form of [`opts`]: the same target, for code that a
/// loader maps as a shared object.
///
/// This exists as a whole target rather than as a `-C` flag because what a
/// `cdylib` needs is not a codegen switch on the crate being built: it is the
/// `core` and `alloc` it links against, which must be compiled without the
/// absolute relocations a shared object cannot carry (`R_X86_64_64 cannot be
/// used against local symbol`). So this target's sysroot is built PIC, and the
/// executable form keeps its `static` model so nothing else in the port changes.
///
/// It deliberately carries no `--image-base`: the loader maps a shared object at
/// a base it chooses (`slot + p_vaddr`), so a link-time base would leave the slot
/// it reserved short by the object's own base.
pub(crate) fn dyn_opts() -> TargetOptions {
    minix_opts(true, &[])
}

fn minix_opts(position_independent: bool, pre_link_args: &'static [&'static str]) -> TargetOptions {
    let pre_link_args = TargetOptions::link_args(LinkerFlavor::Gnu(Cc::No, Lld::No), pre_link_args);

    TargetOptions {
        os: Os::Minix,
        executables: true,
        // Real ELF TLS (`#[thread_local]` statics in a PT_TLS segment): the
        // kernel exposes a per-thread thread pointer (`SYS_thread_set_tls`,
        // FS base / tpidr_el0 / tp) that `sys::thread_local` uses, and the
        // runtime (`minix_start` and the thread trampoline) allocates a TLS
        // block per thread from the linker-provided `__tls_start`/`__tls_end`.
        has_thread_local: true,
        // 1:1 kernel threads: the kernel schedules threads as Proc slots
        // (`thread_create`/`join`/`yield`/`set_tls`, futex wait/wake). This
        // clears `cfg(target_has_threads)`.
        singlethread: false,
        // The std PAL entry point (`_start`) reads argc/argv off the initial
        // stack, so rustc generates a `main(argc, argv)` entry wrapper that
        // calls the `start` lang item.
        main_needs_argc_argv: true,
        panic_strategy: PanicStrategy::Abort,
        linker_flavor: LinkerFlavor::Gnu(Cc::No, Lld::Yes),
        pre_link_args,
        // Fully static binaries: no external CRT. A shared object still needs the
        // CRT to be absent, but `crt_static_allows_dylibs` is what keeps rustc
        // from dropping the `cdylib` crate type outright, and rustc requires
        // `dynamic_linking` wherever it is set — as it requires `pic` wherever
        // dynamic linking is allowed.
        crt_static_default: true,
        crt_static_respected: true,
        crt_static_allows_dylibs: position_independent,
        dynamic_linking: position_independent,
        // Match the existing `*-minix` JSON targets used by the kernel.
        relocation_model: if position_independent { RelocModel::Pic } else { RelocModel::Static },
        disable_redzone: true,
        ..Default::default()
    }
}
