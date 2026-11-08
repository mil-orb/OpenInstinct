// Minimal compiler_builtins replacement: only the mem functions LLVM may emit.
#![no_std]
#![feature(compiler_builtins)]
#![compiler_builtins]
#![no_builtins]
#![allow(internal_features)]

#[no_mangle]
pub unsafe extern "C" fn memcpy(d: *mut u8, s: *const u8, n: usize) -> *mut u8 {
    let mut i = 0; while i < n { *d.add(i) = *s.add(i); i += 1; } d
}
#[no_mangle]
pub unsafe extern "C" fn memmove(d: *mut u8, s: *const u8, n: usize) -> *mut u8 {
    if (d as usize) < (s as usize) { memcpy(d, s, n) } else {
        let mut i = n; while i > 0 { i -= 1; *d.add(i) = *s.add(i); } d }
}
#[no_mangle]
pub unsafe extern "C" fn memset(d: *mut u8, c: i32, n: usize) -> *mut u8 {
    let mut i = 0; while i < n { *d.add(i) = c as u8; i += 1; } d
}
#[no_mangle]
pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut i = 0; while i < n { let x = *a.add(i); let y = *b.add(i);
        if x != y { return x as i32 - y as i32; } i += 1; } 0
}
#[no_mangle]
pub unsafe extern "C" fn bcmp(a: *const u8, b: *const u8, n: usize) -> i32 { memcmp(a, b, n) }

// ARM EABI helpers LLVM emits for Thumb targets. Note __aeabi_memset's
// (dest, n, c) argument order.
#[no_mangle] pub unsafe extern "C" fn __aeabi_memcpy(d: *mut u8, s: *const u8, n: usize) { memcpy(d, s, n); }
#[no_mangle] pub unsafe extern "C" fn __aeabi_memcpy4(d: *mut u8, s: *const u8, n: usize) { memcpy(d, s, n); }
#[no_mangle] pub unsafe extern "C" fn __aeabi_memcpy8(d: *mut u8, s: *const u8, n: usize) { memcpy(d, s, n); }
#[no_mangle] pub unsafe extern "C" fn __aeabi_memmove(d: *mut u8, s: *const u8, n: usize) { memmove(d, s, n); }
#[no_mangle] pub unsafe extern "C" fn __aeabi_memset(d: *mut u8, n: usize, c: i32) { memset(d, c, n); }
#[no_mangle] pub unsafe extern "C" fn __aeabi_memset4(d: *mut u8, n: usize, c: i32) { memset(d, c, n); }
#[no_mangle] pub unsafe extern "C" fn __aeabi_memclr(d: *mut u8, n: usize) { memset(d, 0, n); }
#[no_mangle] pub unsafe extern "C" fn __aeabi_memclr4(d: *mut u8, n: usize) { memset(d, 0, n); }
#[no_mangle] pub unsafe extern "C" fn __aeabi_memclr8(d: *mut u8, n: usize) { memset(d, 0, n); }
