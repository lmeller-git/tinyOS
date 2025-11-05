use alloc::{boxed::Box, vec, vec::Vec};
use core::{str, sync::atomic::Ordering, time::Duration};

use tinyos_abi::{
    flags::{OpenOptions, PageTableFlags, TaskStateChange, TaskWaitOptions, WaitOptions},
    types::{FileDescriptor, SysCallRes, SysRetCode},
};

use crate::{
    arch::{
        interrupt::gdt::get_kernel_selectors,
        mem::{PageSize, Size4KiB, VirtAddr},
        x86::current_time,
    },
    drivers::wait_manager::{add_queue, remove_queue, wait_self},
    eprintln,
    kernel::{
        abi::syscalls::utils::{__sys_yield, valid_ptr},
        fd::FileRepr,
        fs::{self, Path},
        io::Read,
        mem::{
            align_up,
            paging::{map_region, map_region_into, unmap_region},
        },
        threading::{
            self,
            schedule::{self, add_built_task},
            task::{TaskBuilder, TaskRepr, TaskState},
            tls,
            trampoline::TaskExitInfo,
            wait::{
                QueuTypeCondition,
                QueueHandle,
                QueueType,
                WaitEvent,
                condition::WaitCondition,
                post_event,
                queues::{GenericWaitQueue, WaitQueue},
            },
        },
    },
    println,
    serial_print,
    serial_println,
};

// all lengths denote the number of ELEMENTS, not the number of bytes.

// TODO we should likely check if the corresponding file is already open in the task. If this is true, we should hand out the corresponding fd.
// However this necessitates that we also store the Path either in File or in FDMap.
pub fn open(path: *const u8, len: usize, flags: OpenOptions) -> SysCallRes<FileDescriptor> {
    if !valid_ptr(path, len) {
        return Err(SysRetCode::Fail);
    }
    let p = unsafe { str::from_raw_parts(path, len) };
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

pub fn read(fd: FileDescriptor, buf: *mut u8, len: usize, timeout: i64) -> SysCallRes<isize> {
    if !valid_ptr(buf, len) {
        return Err(SysRetCode::Fail);
    }
    let current_task = tls::task_data().get_current().ok_or(SysRetCode::Fail)?;
    let b = unsafe { &mut *core::ptr::slice_from_raw_parts_mut(buf, len) };
    let n = current_task
        .fd(fd)
        .map(|f| f.read_continuous(b).ok())
        .flatten()
        .ok_or(SysRetCode::Fail)?;
    if n > 0 || timeout == 0 {
        return Ok(n as isize);
    }

    // fast path failed, we will now try to wait until timeout
    // OR until the watched file is updated, if the path is known
    let mut conditions = Vec::new();
    let until = Duration::from_millis(timeout as u64) + current_time();
    if timeout > 0 {
        conditions.push(QueuTypeCondition::with_cond(
            QueueType::Timer,
            WaitCondition::Time(until),
        ));
    }
    if let Some(path) = current_task.fd(fd).ok_or(SysRetCode::Fail)?.get_path() {
        conditions.push(QueuTypeCondition::new(QueueType::file(path)));
        add_queue(
            QueueHandle::from_owned(Box::new(GenericWaitQueue::new()) as Box<dyn WaitQueue>),
            QueueType::file(path),
        );
    }

    if conditions.is_empty() {
        return Err(SysRetCode::Fail);
    }

    loop {
        let n = current_task
            .fd(fd)
            .map(|f| f.read_continuous(b).ok())
            .flatten()
            .ok_or(SysRetCode::Fail)?;

        if n == 0 && until > current_time() {
            wait_self(&conditions).unwrap();
        } else {
            // TODO we do not want to do this for EVERY queue. Some files (like keyboard) may be queried very often.
            // These should persist
            // if let Some(path) = current_task.fd(fd).ok_or(SysRetCode::Fail)?.get_path() {
            //     remove_queue(&QueueType::file(path));
            // }
            return Ok(n as isize);
        }
    }
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

pub fn dup(old_fd: FileDescriptor, new_fd: i32) -> SysCallRes<FileDescriptor> {
    let current = tls::task_data().get_current().ok_or(SysRetCode::Fail)?;
    let next_fd = if new_fd >= 0 {
        new_fd as FileDescriptor
    } else {
        current.next_fd()
    };

    let old = current.fd(old_fd).ok_or(SysRetCode::Fail)?;
    current.add_fd(next_fd, old);
    Ok(next_fd)
}

pub fn yield_now() -> SysCallRes<()> {
    let (cs, ss) = get_kernel_selectors();
    unsafe {
        __sys_yield(cs.0 as u64, ss.0 as u64);
    }
    Ok(())
}

pub fn exit(status: i64) -> ! {
    post_event(WaitEvent::with_data(
        QueueType::Thread(tls::task_data().current_pid()),
        TaskStateChange::EXIT.bits() as u64,
    ));

    tls::task_data().kill(&tls::task_data().current_pid(), 0);
    threading::yield_now();
    unreachable!("task did not exit properly");
}

pub fn kill(pid: u64, signal: i64) -> SysCallRes<()> {
    tls::task_data()
        .kill(&pid.into(), signal as i32)
        .ok_or(SysRetCode::Fail)
}

pub fn mmap(len: usize, addr: *mut u8, flags: PageTableFlags, fd: i32) -> SysCallRes<*mut u8> {
    // TODO add a more sophisticated approach for managing address spaces
    let current = tls::task_data().get_current().ok_or(SysRetCode::Fail)?;
    let addr = if !valid_ptr(addr, len) {
        serial_println!("assigning new mmap ptr");
        current
            .next_addr()
            .fetch_update(Ordering::Release, Ordering::Acquire, |addr_| {
                // we need to store addr_.align_up() + len, as this is what will get mapped
                Some(align_up(addr_, Size4KiB::SIZE as usize) + len)
            })
            .unwrap() as *mut u8
    } else {
        addr
    };

    let base_addr = VirtAddr::from_ptr(addr).align_up(Size4KiB::SIZE);
    serial_println!("mmap at addr {:#x}", base_addr.as_u64());

    if fd >= 0 {
        // map file stored at fd into memory.
        // as the file is opened already, the mapping already exists in this address space.
        // we must copy it to the specified user accesible address
        let (from, true_len) = current
            .fd(fd as FileDescriptor)
            .ok_or(SysRetCode::Fail)?
            .as_raw_parts();

        serial_println!(
            "trying to map file to addr {:#x}, from {:#x}",
            base_addr,
            from as usize
        );
        match map_region_into(
            base_addr,
            len.min(true_len),
            flags,
            current.pagedir(),
            VirtAddr::from_ptr(from),
            current.pagedir(),
        ) {
            Err(e) => {
                eprintln!("failed to map file: {}", e);
                current.next_addr().compare_exchange(
                    addr as usize,
                    align_up(addr as usize, Size4KiB::SIZE as usize) + len,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                );
                return Err(SysRetCode::Fail);
            }
            Ok(v) => {
                serial_println!("the addr is: {:#x}", v);
                return Ok(v.as_mut_ptr());
            }
        }
    } else {
        serial_println!(
            "called anonymous mmap at addr {:#x} with len {}",
            base_addr.as_u64(),
            len
        );
        // map new (anonymous) region initialized with 0
        if let Err(e) = map_region(
            base_addr,
            len,
            flags | PageTableFlags::PRESENT,
            current.pagedir(),
        ) {
            serial_println!("got an err during mmmap: {:?}", e);
            // try to free space in task mmmap space again
            current.next_addr().compare_exchange(
                addr as usize,
                align_up(addr as usize, Size4KiB::SIZE as usize) + len,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );
            return Err(SysRetCode::Fail);
        }
    }
    Ok(base_addr.as_mut_ptr())
}

pub fn munmap(addr: *mut u8, len: usize) -> SysCallRes<()> {
    // TODO this should free the underlying memory iff it was anonmyously mapped, ie iff it is not shared elsewhere
    if !valid_ptr(addr, len) {
        return Err(SysRetCode::Fail);
    }

    let base = VirtAddr::from_ptr(addr).align_up(Size4KiB::SIZE);
    let current = tls::task_data().get_current().ok_or(SysRetCode::Fail)?;

    unmap_region(base, len, current.pagedir()).map_err(|_| SysRetCode::Fail)
}

// TODO handle args
/// spawns a new thread in a new address space.
pub fn spawn(elf_data: *const u8, len: usize) -> SysCallRes<()> {
    if !valid_ptr(elf_data, len) {
        return Err(SysRetCode::Fail);
    }
    let bytes = unsafe { &*core::ptr::slice_from_raw_parts(elf_data, len) };
    let task = TaskBuilder::from_bytes(bytes)
        .map_err(|_| SysRetCode::Fail)?
        .with_default_files(false)
        .as_usr()
        .map_err(|_| SysRetCode::Fail)?
        .build();
    schedule::add_built_task(task);

    Ok(())
}

pub fn waittime(duration: u64) -> SysCallRes<()> {
    let conditions = &[QueuTypeCondition::with_cond(
        QueueType::Timer,
        WaitCondition::Time(Duration::from_millis(duration) + current_time()),
    )];
    wait_self(conditions).ok_or(SysRetCode::Fail)
}

pub fn wait_pid(
    id: u64,
    timeout: i64,
    w_flags: WaitOptions,
    tw_flags: TaskWaitOptions,
) -> SysCallRes<TaskStateChange> {
    let task = tls::task_data().get(&id.into()).ok_or(SysRetCode::Fail)?;
    if timeout == 0 {
        return Ok(TaskStateChange::empty());
    }
    let mut conditions = Vec::new();
    if timeout > 0 {
        let until = Duration::from_millis(timeout as u64) + current_time();
        conditions.push(QueuTypeCondition::with_cond(
            QueueType::Timer,
            WaitCondition::Time(until),
        ));
    }
    conditions.push(QueuTypeCondition::with_cond(
        QueueType::Thread(id.into()),
        WaitCondition::Thread(id.into(), tw_flags),
    ));

    if w_flags.contains(WaitOptions::NOBLOCK) {
        todo!()
    }
    let q_type = QueueType::Thread(id.into());
    add_queue(
        QueueHandle::from_owned(Box::new(GenericWaitQueue::new()) as Box<dyn WaitQueue>),
        q_type.clone(),
    );

    let r = wait_self(&conditions)
        .ok_or(SysRetCode::Fail)
        .map(|_| match task.state() {
            TaskState::Running | TaskState::Ready => TaskStateChange::WAKEUP,
            TaskState::Blocking | TaskState::Sleeping => TaskStateChange::BLOCK,
            TaskState::Zombie => TaskStateChange::EXIT,
        });
    remove_queue(&q_type);
    r
}

pub fn eventfd() -> SysCallRes<FileDescriptor> {
    todo!()
}

pub fn machine() -> SysCallRes<()> {
    todo!()
}

pub fn get_pid() -> SysCallRes<u64> {
    Ok(tls::task_data().current_pid().get_inner())
}

pub fn serial(buf: *const u8, len: usize) -> SysCallRes<()> {
    if !valid_ptr(buf, len) {
        return Err(SysRetCode::Fail);
    }
    let str = unsafe { str::from_raw_parts(buf, len) };
    serial_print!("{}", str);
    Ok(())
}

pub fn fork() -> SysCallRes<bool> {
    // procedure:
    // - copy relevant structures from current task into new task (devices, privilege, ....)
    // - create a new stack for the new task and copy all contents of the old task into it (including current interrupt frame, saved state)
    // - modify the interrupt frame, such that the syscall returns true (1) for the new task in RAX. The old task will receive false (0) in RAX.
    // - add the new task to task data
    // - sysret
    let current_task = tls::task_data().get_current().ok_or(SysRetCode::Fail)?;
    Err(SysRetCode::Fail)
}

pub fn execve(path: *const u8, len: usize) -> SysCallRes<u64> {
    if !valid_ptr(path, len) {
        return Err(SysRetCode::Fail);
    }
    let path = unsafe { str::from_raw_parts(path, len) };
    serial_println!("called execve on {}", path);

    let bin = fs::open(Path::new(path), OpenOptions::READ).map_err(|_| SysRetCode::Fail)?;
    let mut buf = Vec::new();
    let bytes = bin.read_to_end(&mut buf, 0).map_err(|_| SysRetCode::Fail)?;
    serial_println!("the file exists. read {} bytes", bytes);
    let current = tls::task_data().get_current().ok_or(SysRetCode::Fail)?;
    let mut new = TaskBuilder::from_bytes(&buf[..bytes])
        .map_err(|_| SysRetCode::Fail)?
        .with_default_files(false)
        // .override_files(
        //     current
        //         .metadata
        //         .fd_table
        //         .read()
        //         .iter()
        //         .map(|(k, f)| (*k, f.clone())),
        // )
        ;
    let new = new.as_usr().unwrap().build(); //.map_err(|_| SysRetCode::Fail)?.build();
    let id = new.pid().get_inner();
    add_built_task(new);
    serial_println!("the task was spawned");
    Ok(id)
}

// TODO implement this at some point
pub fn pthread_create() -> SysCallRes<u64> {
    todo!()
}

pub fn pthread_exit() -> ! {
    todo!()
}

pub fn pthread_cancel(id: u64) -> SysCallRes<i64> {
    todo!()
}

pub fn pthread_join(id: u64, timeout: i64) -> SysCallRes<i64> {
    todo!()
}

pub fn time() -> SysCallRes<u64> {
    // TODO this should return a u128, but this requires splitting across registers / ptr
    Ok(current_time().as_millis() as u64)
}
