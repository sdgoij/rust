pub fn is_terminal<T>(_: &T) -> bool {
    // The kernel does not expose a `ttyname`/`isatty` syscall yet.
    false
}
