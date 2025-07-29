use core::{arch::global_asm, array, ptr::null_mut};

use crate::{
    add_device,
    arch::{
        interrupt::{
            self,
            gdt::{get_kernel_selectors, get_user_selectors},
        },
        mem::VirtAddr,
    },
    drivers::graphics::{
        framebuffers::{get_config, BoundingBox, FrameBuffer}, GLOBAL_FRAMEBUFFER
    },
    get_device,
    kernel::{
        devices::{
            tty::io::read_all, DeviceBuilder, FdEntry, FdEntryType, GraphicsTag, RawDeviceID, RawFdEntry
        },
        mem::{
            align_up, heap::{MAX_USER_HEAP_SIZE, USER_HEAP_START}, paging::GLOBAL_FRAME_ALLOCATOR
        },
        threading::{
            self,
            schedule::{
                self, context_switch_local, current_task, with_current_task, with_scheduler, OneOneScheduler
            },
            task::{PrivilegeLevel, TaskID, TaskRepr},
            yield_now,
        },
    },
    serial_println,
};

use super::SysRetCode;

const USER_DEVICE_MAP: VirtAddr = VirtAddr::new(0x0000_3000_0000);

pub fn sys_exit(status: i64) {
    with_current_task(|task| task.with_inner_mut(|task| task.kill_with_code(status as usize)));

    yield_now();
}

pub fn sys_kill(id: u64, status: i64) -> SysRetCode {
    with_scheduler(|sched| sched.lock().kill(id.into()));
    SysRetCode::Success
}

pub fn sys_yield() -> SysRetCode {
    let (cs, ss) = get_kernel_selectors();
    unsafe {
        __sys_yield(cs.0 as u64, ss.0 as u64);
    }
    SysRetCode::Success
}

pub fn sys_write(device_type: usize, buf: *const u8, len: usize) -> isize {
    // device_type maps 1:1 to FdEntryType
    // -1: device type not writeable
    // -2: no device available or device type not writeable
    // -3: device list cannot be accessed??
    let Ok(entry_type) = FdEntryType::try_from(device_type) else {
        return -1;
    };

    match entry_type {
        FdEntryType::StdOut | FdEntryType::StdErr | FdEntryType::DebugSink => {
            get_device!(entry_type, RawFdEntry::TTYSink(sinks) => {
                let bytes = unsafe {&*core::ptr::slice_from_raw_parts(buf, len)};
                for device in sinks.values() {
                    device.write(bytes);
                }
                len as isize
            } | {
                -2
            })
            .unwrap_or(-3)
        }
        FdEntryType::Graphics => {
            serial_println!("graphics write");
            get_device!(entry_type, RawFdEntry::GraphicsBackend(id, device) => {
                let bounds = unsafe {&*core::ptr::slice_from_raw_parts(buf as *const BoundingBox, len)};
                for bound in bounds {
                    device.flush(bound);
                }
            
                len as isize
            } | {-2})
            .unwrap_or(-3)
        }
        _ => todo!(),
    }
}

pub fn sys_write_single(device_type: usize, device_id: u64, buf: *const u8, len: usize) -> isize {
    // device_type maps 1:1 to FdEntryType
    // -1: device type not writeable
    // -2: no device available or device type not writeable
    // -3: device list cannot be accessed??
    let Ok(entry_type) = FdEntryType::try_from(device_type) else {
        return -1;
    };

    get_device!(entry_type, RawFdEntry::TTYSink(sinks) => {
        let id: RawDeviceID = device_id.into();
        if let Some(device) = sinks.get(&id) {
            let bytes = unsafe {&*core::ptr::slice_from_raw_parts(buf, len)};
            device.write(bytes);
            len as isize
        } else {
            -2
        }
        } | {
        -2
    })
    .unwrap_or(-3)
}

pub fn sys_read(device_type: usize, buf: *mut u8, len: usize) -> isize {
    // device_type maps 1:1 to FdEntryType
    // -1: device type not writeable
    // -2: no device available or device type not writeable
    // -3: device list cannot be accessed??
    // currently defaults to StdIn and ignores device_type
    let Ok(entry_type) = FdEntryType::try_from(device_type) else {
        return -1;
    };
    let bytes = unsafe { &mut *core::ptr::slice_from_raw_parts_mut(buf, len) };
    read_all(bytes) as isize
}

pub fn sys_heap(size: usize) -> *mut u8 {
    use crate::arch::mem::{
        FrameAllocator, Mapper, Page, PageSize, PageTableFlags, Size4KiB, VirtAddr,
    };

    let current_size = current_task().unwrap().raw().read().heap_size;
    if current_size + size > MAX_USER_HEAP_SIZE {
        return null_mut();
    }
    let base_addr = VirtAddr::new((USER_HEAP_START + current_size) as u64).align_up(Size4KiB::SIZE);
    let end_addr = (base_addr + size as u64).align_up(Size4KiB::SIZE);
    let start_page: Page<Size4KiB> = Page::containing_address(base_addr);
    let end_page: Page<Size4KiB> = Page::containing_address(end_addr);

    with_current_task(|task| {
        task.with_inner_mut(|task| {
            let flags = PageTableFlags::PRESENT
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::WRITABLE;
            let pagedir = task.mut_pagdir();
            let mut alloc = GLOBAL_FRAME_ALLOCATOR.lock();
            for page in Page::range_inclusive(start_page, end_page) {
                if pagedir.table.translate_page(page).is_ok() {
                    continue;
                }
                let frame = alloc.allocate_frame().unwrap();
                unsafe { pagedir.table.map_to(page, frame, flags, &mut *alloc) }
                    .unwrap()
                    .flush();
            }
            task.heap_size += size;
        })
    });
    base_addr.as_mut_ptr()
}

pub fn sys_map_device(device_type: usize, addr: *mut ()) -> Result<*mut (), SysRetCode> {
    let Ok(entry_type) = FdEntryType::try_from(device_type) else {
        return Err(SysRetCode::Fail);
    };

    // TODO dynamically determine this addr + discriminate between user + kernel
    // needs some thread local memmap
    let addr = if addr.is_null() {
        USER_DEVICE_MAP.as_mut_ptr()
    } else {
        addr
    };

    match entry_type {
        FdEntryType::Graphics => {
            let addr = VirtAddr::from_ptr(addr);
            serial_println!("building fb");
            let entry: FdEntry<GraphicsTag> = DeviceBuilder::gfx().blit_user(addr);
            with_current_task(|task| task.raw().write().devices.attach(entry));
        }
        _ => todo!(),
    }

    Ok(addr)
}

// TEMP: This is used to pass config until fs is ready
// TODO: port this into fs, such that configs can be queried from special files/ dirs
#[deprecated]
#[repr(C)]
pub struct GFXConfig {
    pub red_mask_shift: u8,
    pub red_mask_size: u8,
    pub green_mask_shift: u8,
    pub green_mask_size: u8,
    pub blue_mask_shift: u8,
    pub blue_mask_size: u8,
    pub bpp: u16,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
}

impl GFXConfig {
    fn apply_defaults(&mut self) {
        let config = get_config();
        self.red_mask_shift = config.red_mask_shift;
        self.red_mask_size = config.red_mask_size;
        self.green_mask_shift = config.green_mask_shift;
        self.green_mask_size = config.green_mask_size;
        self.blue_mask_shift = config.blue_mask_shift;
        self.blue_mask_size = config.blue_mask_size;
        self.bpp = GLOBAL_FRAMEBUFFER.bpp();
        self.width = GLOBAL_FRAMEBUFFER.width() as u32;
        self.height = GLOBAL_FRAMEBUFFER.height() as u32;
        self.pitch = GLOBAL_FRAMEBUFFER.pitch() as u32;
    }
}

#[deprecated]
pub fn sys_gfx_config(config: *mut GFXConfig) {
    unsafe { &mut *config }.apply_defaults();
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
    fn __sys_yield(cs: u64, ss: u64);
}

#[unsafe(no_mangle)]
extern "C" fn call_context_switch(rsp: u64) {
    unsafe {
        context_switch_local(rsp);
    }
}
