use crate::spec::{Cc, LinkerFlavor, Lld, Os, PanicStrategy, RelocModel, TargetOptions};

pub(crate) fn opts() -> TargetOptions {
    // Pin the image base at the OS's userland address. lld's default base
    // (0x200000) overlaps the kernel itself (kmain @ 0x200000), so an
    // unlinked binary would silently alias kernel memory at runtime. This
    // mirrors `BASE_ADDRESS` in the OS's `tools/minix-user.ld`; the kernel's
    // exec loader derives the code span from the lowest PT_LOAD, so the
    // headers segment must land here too (hence `--image-base`, not just
    // `-Ttext`).
    let pre_link_args =
        TargetOptions::link_args(LinkerFlavor::Gnu(Cc::No, Lld::No), &["--image-base=0x1000000"]);

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
        // Fully static binaries: no dynamic linking, no external CRT.
        crt_static_default: true,
        crt_static_respected: true,
        dynamic_linking: false,
        // Match the existing `*-minix` JSON targets used by the kernel.
        relocation_model: RelocModel::Static,
        disable_redzone: true,
        ..Default::default()
    }
}
