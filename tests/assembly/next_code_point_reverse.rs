//@ assembly-output: emit-asm
//@ compile-flags: --target aarch64-unknown-linux-gnu
//@ needs-llvm-components: aarch64

#![crate_type = "rlib"]

#[no_mangle]
fn next_code_point_reverse(s: &str) -> Option<char> {
    s.chars().rev().next()
    // CHECK-LABEL: next_code_point_reverse:
    // CHECK-NEXT: .cfi_startproc
    // CHECK-NEXT: cbz x1, .LBB0_3
    // CHECK-NEXT: add x8, x0, x1
    // CHECK-NEXT: ldursb w0, [x8, #-1]
    // CHECK-NEXT: tbnz w0, #31, .LBB0_4
    // CHECK-NEXT: ret
    // CHECK-NEXT: .LBB0_3:
    // CHECK-NEXT: mov w0, #1114112
    // CHECK-NEXT: ret
    // CHECK-NEXT: .LBB0_4:
    // CHECK-NEXT: ldursb w9, [x8, #-2]
    // CHECK-NEXT: cmn w9, #64
    // CHECK-NEXT: b.ge .LBB0_7
    // CHECK-NEXT: ldursb w10, [x8, #-3]
    // CHECK-NEXT: and w9, w9, #0xff
    // CHECK-NEXT: cmn w10, #64
    // CHECK-NEXT: b.ge .LBB0_8
    // CHECK-NEXT: and w10, w10, #0xff
    // CHECK-NEXT: ldurb w11, [x8, #-4]
    // CHECK-NEXT: and w8, w10, #0x3f
    // CHECK-NEXT: bfi w8, w11, #6, #3
    // CHECK-NEXT: b .LBB0_9
    // CHECK-NEXT: .LBB0_7:
    // CHECK-NEXT: and w8, w9, #0x1f
    // CHECK-NEXT: b .LBB0_10
    // CHECK-NEXT: .LBB0_8:
    // CHECK-NEXT: and w8, w10, #0xf
    // CHECK-NEXT: .LBB0_9:
    // CHECK-NEXT: and w9, w9, #0x3f
    // CHECK-NEXT: orr w8, w9, w8, lsl #6
    // CHECK-NEXT: .LBB0_10:
    // CHECK-NEXT: and w9, w0, #0x3f
    // CHECK-NEXT: orr w0, w9, w8, lsl #6
    // CHECK-NEXT: ret
    // CHECK-NEXT: .Lfunc_end0:
}
