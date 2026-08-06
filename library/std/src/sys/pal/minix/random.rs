use crate::ptr;

// There is no kernel entropy source on Minix yet, so `fill_bytes` is not
// available. `hashmap_random_keys` uses allocation addresses, mirroring the
// `unsupported` fallback used by wasm/xous.

pub fn fill_bytes(_: &mut [u8]) {
    panic!("this target does not support random data generation");
}

pub fn hashmap_random_keys() -> (u64, u64) {
    let stack = 0u8;
    let heap = Box::new(0u8);
    let k1 = ptr::from_ref(&stack).addr() as u64;
    let k2 = ptr::from_ref(&*heap).addr() as u64;
    (k1, k2)
}
