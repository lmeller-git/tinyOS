use core::{
    arch::{asm, global_asm},
    hint::spin_loop,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};

use x86_64::structures::{DescriptorTablePointer, gdt::GlobalDescriptorTable, paging::PageTable};

use crate::{
    arch::{
        context::allocate_kstack,
        hcf,
        interrupt::gdt::GDT,
        mem::{PhysAddr, VirtAddr},
    },
    kernel::mem::paging::PAGETABLE,
    serial_println,
};

//TODO rewrite all

static AP_RUNNING: AtomicU8 = AtomicU8::new(0);
static BSP_DONE: AtomicBool = AtomicBool::new(false);

// TODO
const AP_STACK_TOP: usize = 0x90000;

pub fn bsp_is_done() {
    BSP_DONE.store(true, Ordering::Release);
}

#[unsafe(no_mangle)]
pub extern "C" fn ap_main(apic_id: u32) -> ! {
    serial_println!("hello worl from ap {apic_id}");
    hcf()
}

#[unsafe(no_mangle)]
extern "C" fn ap_startup_(apic_id: u32) {
    // enable paging
    let addr = PAGETABLE.lock().level_4_table() as *const PageTable as usize;
    unsafe {
        asm!("mov cr3, {}", in(reg) addr);

        asm!(
            "mov {tmp}, cr0",
            "or {tmp}, {paging_bit}",
            "mov cr0, {tmp}",
            tmp = out(reg) _,
            paging_bit = in(reg) 1u64 << 31
        );
    }

    // load gdt
    // TODO: per ap GDT
    unsafe { GDT.get().unwrap().0.load_unsafe() };

    // setup aps stack
    // TODO kstacks are meant for tasks, this could use a more simple solution
    let stack_top = allocate_kstack().unwrap().as_u64();
    unsafe {
        asm!("mov rsp, {}", in(reg) stack_top);
    }

    while !BSP_DONE.load(Ordering::Acquire) {
        spin_loop();
    }

    AP_RUNNING.fetch_add(1, Ordering::Release);

    ap_main(apic_id)
}

global_asm!(
    "
        .global ap_trampoline_

        .code16
        ap_trampoline_:
            cli
            cld

            xor ax, ax
            mov ds, ax
            mov es, ax
            mov ss, ax

            lgdt [gdt_desc]

            // protected mode
            mov eax, cr0
            or eax, 0x1
            mov cr0, eax

            // jmp 0x08:protected_mode_stub //TODO

        .code32
        protected_mode_stub:
            mov ax, 0x10
            mov ds, ax
            mov es, ax
            mov fs, ax
            mov gs, ax
            mov ss, ax

            // stack
            mov esp, 0x7C00

            // test for long mode
            mov eax, 0x80000000
            cpuid
            cmp eax, 0x80000001
            jb no_long_mode

            mov eax, 0x80000001
            cpuid
            test edx, 1 << 29
            jz no_long_mode

            // paging will be set up in rust, as will the 64 bit gdt and stack
            
            // PAE
            mov eax, cr4
            or eax, 1 << 5
            mov cr4, eax

            // long mode
            mov ecx, 0xC0000080
            rdmsr
            or eax, 1 << 8
            wrmsr

            // do not yet load gdt, this will be doen in rust
            // jmp 0x08:long_mode_stub //TODO 

        // .code32
        // compatibility_mode_stub:
        //     mov ax, 0x10
        //     mov ds, ax
        //     mov es, ax
        //     mov fs, ax
        //     mov gs, ax
        //     mov ss, ax

        //     mov eax, 1
        //     cpuid
        //     shr ebx, 24
        //     push ebx

        //     call ap_startup_
        //     cli
        //     hlt
            
    
        .code64
        long_mode_stub:
            mov ax, 0x10
            mov ds, ax
            mov es, ax
            mov fs, ax
            mov gs, ax
            mov ss, ax

            mov eax, 1
            cpuid
            shr ebx, 24
            // mov rdi, ebx // apic id TODO

            // call into rust
            call ap_startup_
            cli
            hlt

        no_long_mode:
            cli
            hlt

        .align 16
        gdt_table:
            .long 0, 0                          // null descriptor
            .long 0x0000FFFF, 0x00CF9A00        // flat code
            .long 0x0000FFFF, 0x00CF9200        // flat data
            .long 0x00000068, 0x00CF8900        // tss

        gdt_desc:
            .word gdt_desc - gdt_table - 1
            .long gdt_table
            .long 0, 0
                       
        ap_trampoline_end_:
            
    "
);
