use crate::spec::{Cc, LinkerFlavor, Lld, Os, PanicStrategy, RelocModel, TargetOptions};

pub(crate) fn opts() -> TargetOptions {
    minix_opts(false)
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
pub(crate) fn dyn_opts() -> TargetOptions {
    minix_opts(true)
}

fn minix_opts(position_independent: bool) -> TargetOptions {
    // Pin the image base at the OS's userland address. lld's default base
    // (0x200000) overlaps the kernel itself (kmain @ 0x200000), so an
    // unlinked binary would silently alias kernel memory at runtime. This
    // mirrors `BASE_ADDRESS` in the OS's `tools/minix-user.ld`; the kernel's
    // exec loader derives the code span from the lowest PT_LOAD, so the
    // headers segment must land here too (hence `--image-base`, not just
    // `-Ttext`). A shared object is the exception: the loader maps it at a base
    // it chooses (`slot + p_vaddr`), so a link-time base would leave the slot it
    // reserved short by the object's own base.
    let image_base: &[&'static str] =
        if position_independent { &[] } else { &["--image-base=0x1000000"] };
    let pre_link_args = TargetOptions::link_args(LinkerFlavor::Gnu(Cc::No, Lld::No), image_base);

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
