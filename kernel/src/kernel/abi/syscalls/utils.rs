use core::arch::global_asm;

use crate::kernel::{mem::paging::get_hhdm_addr, threading::schedule::context_switch_local};

/// returns true if the buffer is entirely in user space.
/// len is assumed to be the numebr of ELEMENTS T.
pub fn valid_ptr<T>(ptr: *const T, len: usize) -> bool {
    let base = ptr.addr();
    !ptr.is_null()
        && base < get_hhdm_addr() as usize
        && base + (len * size_of::<T>()) < get_hhdm_addr() as usize
}

global_asm!(
    "
    .global __sys_yield

    __sys_yield:
        mov rax, rsp
        push rsi // ss
        push rax
        pushfq
        push rdi // cs
        lea rax, [rip + _sys_yield_label]
        push rax
        jmp __context_switch_stub

    _sys_yield_label:
        ret

    __context_switch_stub:
            cli
            push rax
            push rbp
            push rdi
            push rsi
            push rdx
            push rcx
            push rbx
            mov rax, cr3
            push rax
            push r15
            push r14
            push r13
            push r12
            push r11
            push r10
            push r9
            push r8

            // save current rsp
            mov r9, rsp

            // align stack, save rsp and save xmm registers
            sub rsp, 512 + 16
            and rsp, -16
            fxsave [rsp]
            push r9

            mov rdi, rsp
            call context_switch_local

            // pop xmm registers
            pop r9
            fxrstor [rsp]
            mov rsp, r9

            pop r8
            pop r9
            pop r10
            pop r11
            pop r12
            pop r13
            pop r14
            pop r15
            pop rax // cr3
            mov cr3, rax // not necessary, as task not switched
            pop rbx
            pop rcx
            pop rdx
            pop rsi
            pop rdi
            pop rbp
            pop rax
            jmp interrupt_cleanup

    "
);

unsafe extern "C" {
    pub fn __sys_yield(cs: u64, ss: u64);
}

#[unsafe(no_mangle)]
pub extern "C" fn call_context_switch(rsp: u64) {
    unsafe {
        context_switch_local(rsp);
    }
}
