use core::arch::global_asm;

use crate::{
    arch::interrupt::gdt::{get_kernel_selectors, get_user_selectors},
    kernel::threading::{
        schedule::{self, OneOneScheduler, context_switch_local, current_task, with_current_task},
        task::{PrivilegeLevel, TaskID, TaskRepr},
        yield_now,
    },
};

use super::SysRetCode;

pub(super) fn sys_exit(status: i64) {
    with_current_task(|task| task.with_inner_mut(|task| task.kill_with_code(status as usize)));

    yield_now();
}

pub(super) fn sys_kill(id: u64, status: i64) -> SysRetCode {
    schedule::get().unwrap().kill(id.into());
    SysRetCode::Success
}

pub(super) fn sys_yield() -> SysRetCode {
    let (cs, ss) = get_kernel_selectors();
    unsafe {
        __sys_yield(cs.0 as u64, ss.0 as u64);
    }
    SysRetCode::Success
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
            mov rdi, rsp
            call context_switch_local
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
    fn __sys_yield(cs: u64, ss: u64);
}

#[unsafe(no_mangle)]
extern "C" fn call_context_switch(rsp: u64) {
    unsafe {
        context_switch_local(rsp);
    }
}
