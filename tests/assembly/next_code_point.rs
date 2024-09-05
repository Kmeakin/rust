//@ assembly-output: emit-asm
//@ compile-flags: --target aarch64-unknown-linux-gnu
//@ needs-llvm-components: aarch64

#![crate_type = "rlib"]

#[no_mangle]
fn next_code_point(s: &str) -> Option<char> {
    s.chars().next()
    // CHECK-LABEL: next_code_point:
    // CHECK-NEXT: .cfi_startproc
    // CHECK-NEXT: cbz x1, .LBB0_3
    // CHECK-NEXT: ldrsb w9, [x0]
    // CHECK-NEXT: mov x8, x0
    // CHECK-NEXT: and w0, w9, #0xff
    // CHECK-NEXT: tbnz w9, #31, .LBB0_4
    // CHECK-NEXT: ret
    // CHECK-NEXT: .LBB0_3:
    // CHECK-NEXT: mov w0, #1114112
    // CHECK-NEXT: ret
    // CHECK-NEXT: .LBB0_4:
    // CHECK-NEXT: ldrb w9, [x8, #1]
    // CHECK-NEXT: cmp w0, #224
    // CHECK-NEXT: and w9, w9, #0x3f
    // CHECK-NEXT: b.lo .LBB0_7
    // CHECK-NEXT: ldrb w10, [x8, #2]
    // CHECK-NEXT: cmp w0, #240
    // CHECK-NEXT: and w10, w10, #0x3f
    // CHECK-NEXT: orr w10, w10, w9, lsl #6
    // CHECK-NEXT: and w9, w0, #0x1f
    // CHECK-NEXT: b.lo .LBB0_8
    // CHECK-NEXT: ldrb w8, [x8, #3]
    // CHECK-NEXT: and w8, w8, #0x3f
    // CHECK-NEXT: orr w0, w8, w10, lsl #6
    // CHECK-NEXT: bfi w0, w9, #18, #3
    // CHECK-NEXT: ret
    // CHECK-NEXT: .LBB0_7:
    // CHECK-NEXT: bfi w9, w0, #6, #5
    // CHECK-NEXT: mov w0, w9
    // CHECK-NEXT: ret
    // CHECK-NEXT: .LBB0_8:
    // CHECK-NEXT: orr w0, w10, w9, lsl #12
    // CHECK-NEXT: ret
    // CHECK-NEXT: .Lfunc_end0:
}
