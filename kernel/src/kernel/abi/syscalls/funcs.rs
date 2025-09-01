use core::{arch::global_asm, ptr::null_mut, sync::atomic::Ordering, time::Duration};

use super::SysRetCode;
use crate::{
    QemuExitCode,
    arch::{interrupt::gdt::get_kernel_selectors, mem::VirtAddr, x86::current_time},
    drivers::{
        graphics::{
            GLOBAL_FRAMEBUFFER,
            framebuffers::{BoundingBox, FrameBuffer, get_config},
        },
        wait_manager::wait_self,
    },
    exit_qemu,
    get_device,
    kernel::{
        devices::{
            DeviceBuilder,
            FdEntry,
            FdEntryType,
            GraphicsTag,
            RawDeviceID,
            RawFdEntry,
            tty::io::read_all,
        },
        mem::{
            heap::{MAX_USER_HEAP_SIZE, USER_HEAP_START},
            paging::map_region,
        },
        threading::{
            schedule::{context_switch_local, with_current_task},
            task::TaskRepr,
            tls,
            wait::{QueuTypeCondition, QueueType, WaitEvent, condition::WaitCondition, post_event},
            yield_now,
        },
    },
    println,
    serial_println,
};

const USER_DEVICE_MAP: VirtAddr = VirtAddr::new(0x0000_3000_0000);

pub fn sys_exit(status: i64) {
    post_event(WaitEvent::new(QueueType::Thread(
        tls::task_data().current_pid(),
    )));
    tls::task_data().kill(&tls::task_data().current_pid(), 0);
    yield_now();
    unreachable!();
}

pub fn sys_kill(id: u64, status: i64) -> SysRetCode {
    tls::task_data().kill(&id.into(), status as i32);
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

pub fn sys_read(device_type: usize, buf: *mut u8, len: usize, timeout: usize) -> isize {
    // device_type maps 1:1 to FdEntryType
    // -1: device type not writeable
    // -2: no device available or device type not writeable
    // -3: device list cannot be accessed??
    // currently defaults to StdIn and ignores device_type
    // reads up to len bytes from stdin and will block until stdin contains at least one byte
    let Ok(entry_type) = FdEntryType::try_from(device_type) else {
        return -1;
    };
    let bytes = unsafe { &mut *core::ptr::slice_from_raw_parts_mut(buf, len) };
    let until = Duration::from_millis(timeout as u64) + current_time();
    let conditions = [
        QueuTypeCondition::with_cond(QueueType::Timer, WaitCondition::Time(until)),
        QueuTypeCondition::with_cond(QueueType::KeyBoard, WaitCondition::Keyboard),
    ];
    loop {
        let r = read_all(bytes);
        if r == 0 && until > current_time() {
            serial_println!(
                "added self to waitqueues until {until:?}, currently: {:?}",
                current_time()
            );
            wait_self(&conditions).unwrap();
        } else {
            serial_println!("returning {r}");
            return r as isize;
        }
    }
}

pub fn sys_heap(size: usize) -> *mut u8 {
    use crate::arch::mem::{PageSize, PageTableFlags, Size4KiB, VirtAddr};
    serial_println!("mapping heap");

    let current = tls::task_data().get_current().unwrap();

    let current_size = current.core.heap_size.load(Ordering::Relaxed);
    if current_size + size > MAX_USER_HEAP_SIZE {
        return null_mut();
    }
    let base_addr = VirtAddr::new((USER_HEAP_START + current_size) as u64).align_up(Size4KiB::SIZE);

    let flags =
        PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::WRITABLE;

    if map_region(
        base_addr,
        size,
        flags,
        &mut *current.pagedir().unwrap().lock().table,
    )
    .is_err()
    {
        return null_mut();
    }

    current.core.heap_size.fetch_add(size, Ordering::Relaxed);
    serial_println!("mapped heap");
    base_addr.as_mut_ptr()
}

pub fn sys_map_device(device_type: usize, addr: *mut ()) -> Result<*mut (), SysRetCode> {
    let Ok(entry_type) = FdEntryType::try_from(device_type) else {
        return Err(SysRetCode::Fail);
    };

    serial_println!("mapping device");

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
            with_current_task(|task| task.devices().write().attach(entry));
        }
        _ => todo!(),
    }
    serial_println!("mapped device");

    Ok(addr)
}

pub fn sys_shutdown() {
    serial_println!("System Shutdown");
    println!("System Shutdown");
    exit_qemu(QemuExitCode::Success);
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
