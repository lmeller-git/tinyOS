use crate::{
    arch::{
        interrupt::gdt::get_kernel_selectors,
        mem::{PageSize, PageTableFlags, Size4KiB, VirtAddr},
    },
    kernel::{
        abi::syscalls::{
            SysCallRes,
            SysRetCode,
            utils::{__sys_yield, valid_ptr},
        },
        fd::FileDescriptor,
        fs::{self, OpenOptions, Path},
        mem::paging::{map_region, unmap_region},
        threading::{
            self,
            task::TaskRepr,
            tls,
            wait::{QueueType, WaitEvent, post_event},
        },
    },
};

// all lengths denote the number of ELEMENTS, not the number of bytes.

pub fn open(path: *const u8, len: usize, flags: OpenOptions) -> SysCallRes<FileDescriptor> {
    if !valid_ptr(path, len) {
        return Err(SysRetCode::Fail);
    }
    let p = unsafe { &*core::ptr::slice_from_raw_parts(path, len) };
    let p = str::from_utf8(p).map_err(|_| SysRetCode::Fail)?;
    let p = Path::new(p);
    Ok(tls::task_data()
        .get_current()
        .ok_or(SysRetCode::Fail)?
        .add_next_file(fs::open(p, flags).map_err(|_| SysRetCode::Fail)?))
}

pub fn close(fd: FileDescriptor) -> SysCallRes<()> {
    tls::task_data()
        .get_current()
        .map(|t| t.remove_fd(fd))
        .flatten()
        .ok_or(SysRetCode::Fail)?;
    Ok(())
}

pub fn read(fd: FileDescriptor, buf: *mut u8, len: usize, timeout: u64) -> SysCallRes<isize> {
    // TODO add wait event till timeout/ read event (may want to put that in userspace though)
    if !valid_ptr(buf, len) {
        return Err(SysRetCode::Fail);
    }
    let b = unsafe { &mut *core::ptr::slice_from_raw_parts_mut(buf, len) };
    let n = tls::task_data()
        .get_current()
        .map(|t| t.fd(fd).map(|f| f.read_continuous(b).ok()))
        .flatten()
        .flatten()
        .ok_or(SysRetCode::Fail)?;
    Ok(n as isize)
}

pub fn write(fd: FileDescriptor, buf: *const u8, len: usize) -> SysCallRes<isize> {
    if !valid_ptr(buf, len) {
        return Err(SysRetCode::Fail);
    }
    let b = unsafe { &*core::ptr::slice_from_raw_parts(buf, len) };
    let n = tls::task_data()
        .get_current()
        .map(|t| t.fd(fd).map(|f| f.write_continuous(b).ok()))
        .flatten()
        .flatten()
        .ok_or(SysRetCode::Fail)?;
    Ok(n as isize)
}

pub fn seek(fd: FileDescriptor, offset: usize) -> SysCallRes<()> {
    tls::task_data()
        .get_current()
        .map(|t| t.fd(fd).map(|f| f.set_cursor(offset)))
        .flatten()
        .ok_or(SysRetCode::Fail)?;
    Ok(())
}

pub fn yield_now() -> SysCallRes<()> {
    let (cs, ss) = get_kernel_selectors();
    unsafe {
        __sys_yield(cs.0 as u64, ss.0 as u64);
    }
    Ok(())
}

pub fn exit(status: i64) -> ! {
    // why do we post an event?????
    // TODO understand this (i believe this is to notify waiting threads of this threads death)
    post_event(WaitEvent::new(QueueType::Thread(
        tls::task_data().current_pid(),
    )));
    tls::task_data().kill(&tls::task_data().current_pid(), 0);
    threading::yield_now();
    unreachable!("task did not exit properly");
}

pub fn kill(pid: u64, signal: i64) -> SysCallRes<()> {
    tls::task_data()
        .kill(&pid.into(), signal as i32)
        .ok_or(SysRetCode::Fail)
}

pub fn mmap(len: usize, addr: *mut u8, flags: PageTableFlags) -> SysCallRes<*mut u8> {
    let addr = if !valid_ptr(addr, len) { todo!() } else { addr };
    let base_addr = VirtAddr::from_ptr(addr).align_up(Size4KiB::SIZE);
    let current = tls::task_data().get_current().ok_or(SysRetCode::Fail)?;

    map_region(
        base_addr,
        len,
        flags,
        &mut *current.pagedir().ok_or(SysRetCode::Fail)?.lock().table,
    )
    .map_err(|_| SysRetCode::Fail)?;
    Ok(base_addr.as_mut_ptr())
}

pub fn munmap(addr: *mut u8, len: usize) -> SysCallRes<()> {
    if !valid_ptr(addr, len) {
        return Err(SysRetCode::Fail);
    }

    let base = VirtAddr::from_ptr(addr).align_up(Size4KiB::SIZE);
    let current = tls::task_data().get_current().ok_or(SysRetCode::Fail)?;

    unmap_region(
        base,
        len,
        &mut *current.pagedir().ok_or(SysRetCode::Fail)?.lock().table,
    )
    .map_err(|_| SysRetCode::Fail)
}

pub fn clone() -> SysCallRes<bool> {
    // procedure:
    // - copy relevant structures from current task into new task (devices, privilege, ....)
    // - create a new stack for the new task and copy all contents of the old task into it (including current interrupt frame, saved state)
    // - modify the interrupt frame, such that the syscall returns true (1) for the new task in RAX. The old task will receive false (0) in RAX.
    // - add the new task to task data
    // - sysret
    let current_task = tls::task_data().get_current().ok_or(SysRetCode::Fail)?;
    Err(SysRetCode::Fail)
}

pub fn wait() -> SysCallRes<()> {
    todo!()
}

pub fn machine() -> SysCallRes<()> {
    todo!()
}

pub fn get_pid() -> SysCallRes<u64> {
    Ok(tls::task_data().current_pid().get_inner())
}
