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
    // CHECK-NEXT: ldursb w9, [x8, #-1]
    // CHECK-NEXT: and w0, w9, #0xff
    // CHECK-NEXT: tbnz w9, #31, .LBB0_4
    // CHECK-NEXT: .LBB0_2:
    // CHECK-NEXT: ret
    // CHECK-NEXT: .LBB0_3:
    // CHECK-NEXT: mov w0, #1114112
    // CHECK-NEXT: ret
    // CHECK-NEXT: .LBB0_4:
    // CHECK-NEXT: ldursb w9, [x8, #-2]
    // CHECK-NEXT: and w0, w0, #0x3f
    // CHECK-NEXT: bfi w0, w9, #6, #6
    // CHECK-NEXT: cmn w9, #64
    // CHECK-NEXT: b.ge .LBB0_2
    // CHECK-NEXT: ldursb w9, [x8, #-3]
    // CHECK-NEXT: bfi w0, w9, #12, #6
    // CHECK-NEXT: cmn w9, #64
    // CHECK-NEXT: b.ge .LBB0_7
    // CHECK-NEXT: ldurb w8, [x8, #-4]
    // CHECK-NEXT: bfi w0, w8, #18, #3
    // CHECK-NEXT: ret
    // CHECK-NEXT: .LBB0_7:
    // CHECK-NEXT: and w0, w0, #0xffff
    // CHECK-NEXT: ret
    // CHECK-NEXT: .Lfunc_end0:
}
