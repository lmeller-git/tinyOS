use alloc::{
    boxed::Box,
    sync::Arc,
    vec::{self, Vec},
};
use core::{str, sync::atomic::Ordering, time::Duration};

use tinyos_abi::{
    flags::{OpenOptions, PageTableFlags, TaskStateChange, TaskWaitOptions, WaitOptions},
    types::{FDAction, FatPtr, FileDescriptor, SysCallRes, SysErrCode},
};

use crate::{
    arch::{
        interrupt::gdt::get_kernel_selectors,
        mem::{PageSize, Size4KiB, VirtAddr},
        x86::current_time,
    },
    args,
    drivers::wait_manager::{add_queue, remove_queue, wait_self},
    eprintln,
    kernel::{
        abi::syscalls::utils::{__sys_yield, valid_ptr},
        devices::tty::Pipe,
        fd::{FPerms, File, FileBuilder, FileRepr},
        fs::{
            self,
            Path,
            PathBuf,
            builtin_bins::{BUILTIN_MARKER, execute},
        },
        io::Read,
        mem::{
            align_up,
            paging::{map_region, map_region_into, unmap_region},
        },
        threading::{
            self,
            schedule::{self, add_built_task, current_task},
            spawn_fn,
            task::{Arg, Args, ProcessID, TaskBuilder, TaskRepr, TaskState},
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
        return Err(SysErrCode::AddrNotValid);
    }
    let p = unsafe { str::from_raw_parts(path, len) };
    let p = Path::new(p);
    Ok(tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?
        .add_next_file(fs::open(p, flags).map_err(|_| SysErrCode::NoFile)?))
}

pub fn close(fd: FileDescriptor) -> SysCallRes<()> {
    tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?
        .remove_fd(fd)
        .ok_or(SysErrCode::BadFd)?;
    Ok(())
}

pub fn read(fd: FileDescriptor, buf: *mut u8, len: usize, timeout: i64) -> SysCallRes<isize> {
    if !valid_ptr(buf, len) {
        return Err(SysErrCode::AddrNotValid);
    }
    let current_task = tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?;
    let b = unsafe { &mut *core::ptr::slice_from_raw_parts_mut(buf, len) };
    let n = current_task
        .fd(fd)
        .ok_or(SysErrCode::BadFd)?
        .read_continuous(b)
        .map_err(|_| SysErrCode::IO)?;
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
    if let Some(cond) = current_task.fd(fd).ok_or(SysErrCode::BadFd)?.get_waiter() {
        conditions.push(cond.clone());
        add_queue(
            QueueHandle::from_owned(Box::new(GenericWaitQueue::new()) as Box<dyn WaitQueue>),
            cond.q_type,
        );
    }

    if conditions.is_empty() {
        return Err(SysErrCode::WouldBlock);
    }

    loop {
        let n = current_task
            .fd(fd)
            .ok_or(SysErrCode::BadFd)?
            .read_continuous(b)
            .map_err(|_| SysErrCode::IO)?;

        if n == 0 && (timeout < 0 || until > current_time()) {
            wait_self(&conditions).ok_or(SysErrCode::WouldBlock)?;
        } else {
            // TODO we do not want to do this for EVERY queue. Some files (like keyboard) may be queried very often.
            // These should persist
            // further we should ensure that concurrent reads on the same queue do not get disrupted by us destroyuing the queue.
            // need refcounting here.
            // TODO add destruction
            // if let Some(path) = current_task.fd(fd).ok_or(SysRetCode::Fail)?.get_path() {
            //     remove_queue(&QueueType::file(path));
            // }
            return Ok(n as isize);
        }
    }
}

pub fn write(fd: FileDescriptor, buf: *const u8, len: usize) -> SysCallRes<isize> {
    if !valid_ptr(buf, len) {
        return Err(SysErrCode::AddrNotValid);
    }
    let b = unsafe { &*core::ptr::slice_from_raw_parts(buf, len) };
    let n = tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?
        .fd(fd)
        .ok_or(SysErrCode::BadFd)?
        .write_continuous(b)
        .map_err(|_| SysErrCode::IO)?;
    Ok(n as isize)
}

pub fn seek(fd: FileDescriptor, offset: usize) -> SysCallRes<()> {
    tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?
        .fd(fd)
        .ok_or(SysErrCode::BadFd)?
        .set_cursor(offset);
    // .ok_or(SysErrCode::InvalidSeek)?;
    Ok(())
}

pub fn dup(old_fd: FileDescriptor, new_fd: i32) -> SysCallRes<FileDescriptor> {
    let current = tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?;
    let next_fd = if new_fd >= 0 {
        new_fd as FileDescriptor
    } else {
        current.next_fd()
    };

    let old = current.fd(old_fd).ok_or(SysErrCode::BadFd)?;
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

// This should kill the current PROCESS
// TODO fix
// --> need process exit first
pub fn exit(status: i64) -> ! {
    post_event(WaitEvent::with_data(
        QueueType::Thread(tls::task_data().current_tid()),
        TaskStateChange::EXIT.bits() as u64,
    ));

    tls::task_data().kill(&tls::task_data().current_tid(), 0);
    threading::yield_now();
    unreachable!("task did not exit properly");
}

// This should kill the specified PROCESS
// TODO fix
// --> need process exit first
pub fn kill(pid: u64, _signal: i64) -> SysCallRes<()> {
    tls::task_data()
        .kill_process(&pid.into())
        .ok_or(SysErrCode::NoProcess)
}

// TODO zero out memory if necessary
pub fn mmap(len: usize, addr: *mut u8, flags: PageTableFlags, fd: i32) -> SysCallRes<*mut u8> {
    // TODO add a more sophisticated approach for managing address spaces
    let current = tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?;
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
            .ok_or(SysErrCode::BadFd)?
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
                return Err(SysErrCode::AddrNotAvail);
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
            return Err(SysErrCode::AddrNotAvail);
        }
    }
    Ok(base_addr.as_mut_ptr())
}

pub fn munmap(addr: *mut u8, len: usize) -> SysCallRes<()> {
    // TODO this should free the underlying memory iff it was anonmyously mapped, ie iff it is not shared elsewhere
    if !valid_ptr(addr, len) {
        return Err(SysErrCode::AddrNotValid);
    }

    let base = VirtAddr::from_ptr(addr).align_up(Size4KiB::SIZE);
    let current = tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?;

    unmap_region(base, len, current.pagedir()).map_err(|_| SysErrCode::AddrNotAvail)
}

// TODO handle args
/// spawns a new thread in a new address space from some provided binary.
pub fn spawn(elf_data: *const u8, len: usize) -> SysCallRes<()> {
    if !valid_ptr(elf_data, len) {
        return Err(SysErrCode::AddrNotValid);
    }
    let bytes = unsafe { &*core::ptr::slice_from_raw_parts(elf_data, len) };
    let task = TaskBuilder::from_bytes(bytes)
        .map_err(|_| SysErrCode::BadMsg)?
        .with_default_files(false)
        .as_usr()
        .map_err(|_| SysErrCode::BadMsg)?
        .build();
    schedule::add_built_task(task);

    Ok(())
}

pub fn waittime(duration: u64) -> SysCallRes<()> {
    let conditions = &[QueuTypeCondition::with_cond(
        QueueType::Timer,
        WaitCondition::Time(Duration::from_millis(duration) + current_time()),
    )];
    wait_self(conditions).ok_or(SysErrCode::WouldBlock)
}

// this should wait for the specified PROCESS to change state.
// TODO
// --> need Process state / exit first
// TODO what do we wait for:
// - process exit
// - any child thread exit?
// - any child exit?
// ...?
// for now just allow W_EXIT
pub fn wait_pid(
    id: u64,
    timeout: i64,
    w_flags: WaitOptions,
    tw_flags: TaskWaitOptions,
) -> SysCallRes<TaskStateChange> {
    if !tw_flags.contains(TaskWaitOptions::W_EXIT) {
        return Err(SysErrCode::Cancelled);
    }
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
        QueueType::Process(id.into()),
        WaitCondition::Generic(
            id.into(),
            Box::into_raw(Box::new(|pid: u64| {
                let state = tls::task_data();
                let processes = state.processes().read();
                let Some(process) = processes.get::<ProcessID>(&pid.into()) else {
                    return true;
                };
                if process.get_process_state() == TaskState::Zombie {
                    true
                } else {
                    false
                }
            })),
        ),
    ));

    if w_flags.contains(WaitOptions::NOBLOCK) {
        return Err(SysErrCode::WouldBlock);
    }
    let q_type = QueueType::Process(id.into());
    add_queue(
        QueueHandle::from_owned(Box::new(GenericWaitQueue::new()) as Box<dyn WaitQueue>),
        q_type.clone(),
    );

    let r = wait_self(&conditions)
        .ok_or(SysErrCode::NoProcess)
        .map(|_| {
            match tls::task_data()
                .processes()
                .read()
                .get::<ProcessID>(&id.into())
                .map(|t| t.get_process_state())
            {
                Some(TaskState::Running) | Some(TaskState::Ready) => TaskStateChange::WAKEUP,
                Some(TaskState::Blocking) | Some(TaskState::Sleeping) => TaskStateChange::BLOCK,
                None | Some(TaskState::Zombie) => TaskStateChange::EXIT,
            }
        });
    remove_queue(&q_type);
    r
}

pub fn eventfd() -> SysCallRes<FileDescriptor> {
    todo!()
}

pub fn get_pid() -> SysCallRes<u64> {
    Ok(tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?
        .pid()
        .0)
}

pub fn serial(buf: *const u8, len: usize) -> SysCallRes<()> {
    if !valid_ptr(buf, len) {
        return Err(SysErrCode::AddrNotValid);
    }
    let str = unsafe { str::from_raw_parts(buf, len) };
    serial_print!("{}", str);
    Ok(())
}

// TODO
pub fn fork() -> SysCallRes<bool> {
    // procedure:
    // - copy relevant structures from current task into new task (devices, privilege, ....)
    // - create a new stack for the new task and copy all contents of the old task into it (including current interrupt frame, saved state)
    // - modify the interrupt frame, such that the syscall returns true (1) for the new task in RAX. The old task will receive false (0) in RAX.
    // - add the new task to task data
    // - sysret
    Err(SysErrCode::OpDenied)
}

pub fn execve(
    path: *const u8,
    len: usize,
    arg: *const FatPtr<u8>,
    env: *const FatPtr<u8>,
) -> SysCallRes<u64> {
    Err(SysErrCode::OpDenied)
}

// essentially posix_spawn
pub fn spawn_process(
    path: *const u8,
    len: usize,
    arg: *const FatPtr<u8>,
    env: *const FatPtr<u8>,
    fd_actions: *const FatPtr<FDAction>,
) -> SysCallRes<u64> {
    if !valid_ptr(path, len)
        || !valid_ptr(arg, 1)
        || !valid_ptr(env, 1)
        || !valid_ptr(fd_actions, 1)
    {
        return Err(SysErrCode::AddrNotValid);
    }

    let arg_data = unsafe { &*arg };
    let env_data = unsafe { &*env };
    let actions = unsafe { &*fd_actions };

    let path = unsafe { str::from_raw_parts(path, len) };
    let bin = fs::open(Path::new(path), OpenOptions::READ).map_err(|_| SysErrCode::NoFile)?;
    let mut buf = Vec::new();
    let bytes = bin.read_to_end(&mut buf, 0).map_err(|_| SysErrCode::IO)?;
    let is_builtin = bytes == BUILTIN_MARKER.len() && &buf[..bytes] == BUILTIN_MARKER;

    // builtin bins (mainly for testing, ...)
    let mut new = if is_builtin {
        // copy args to heap
        let arg_container = if valid_ptr(arg_data.thin, arg_data.size) {
            let slice = unsafe { core::slice::from_raw_parts(arg_data.thin, arg_data.size) };
            Some(slice.to_vec().into_boxed_slice())
        } else {
            None
        };
        let env_container = if valid_ptr(env_data.thin, env_data.size) {
            let slice = unsafe { core::slice::from_raw_parts(env_data.thin, env_data.size) };
            Some(slice.to_vec().into_boxed_slice())
        } else {
            None
        };

        if let Some(v) = &arg_container {
            serial_print!("received {}", unsafe {
                alloc::str::from_boxed_utf8_unchecked(v.clone())
            });
        }

        TaskBuilder::from_fn(execute)
            .map_err(|_| SysErrCode::NoChild)?
            .with_args(args!(
                Path::new(path).to_owned(),
                arg_data.size,
                arg_container,
                env_data.size,
                env_container,
            ))
            .with_default_files(true)
    } else {
        // normal path
        TaskBuilder::from_bytes(&buf[..bytes])
            .map_err(|_| SysErrCode::BadMsg)?
            .with_default_files(true)
    };

    if !actions.thin.is_null() {
        let actions = unsafe { &*core::ptr::slice_from_raw_parts(actions.thin, actions.size) };

        for action in actions {
            match action {
                FDAction::Open(config, fd) => {
                    let path = unsafe { str::from_raw_parts(config.path.thin, config.path.size) };
                    new = new.with_file(
                        *fd,
                        fs::open(Path::new(path), config.flags).map_err(|_| SysErrCode::NoFile)?,
                    );
                }
                FDAction::Close(fd) => new = new.remove_file(*fd),
                FDAction::Dup(from, to) => {
                    let current = new.get_file(*from).ok_or(SysErrCode::NoFile)?;
                    new = new.with_file(*to, current)
                }
                FDAction::Clear => new = new.clear_files(),
                FDAction::Inherit(parent, child) => {
                    let current = current_task()
                        .map_err(|_| SysErrCode::NoProcess)?
                        .fd(*parent)
                        .ok_or(SysErrCode::NoFile)?;
                    new = new.with_file(*child, current);
                }
            }
        }
    }

    let new = if is_builtin {
        new.as_kernel().map_err(|_| SysErrCode::Cancelled)?.build()
    } else {
        new.as_usr()
            .map_err(|_| SysErrCode::Cancelled)?
            .allocate_arg_env(arg_data.size, arg_data.thin, env_data.size, env_data.thin)
            .build()
    };

    let id = new.pid().0;
    add_built_task(new);
    Ok(id)
}

pub fn thread_create(start_routine: *const (), args: *const ()) -> SysCallRes<u64> {
    if !valid_ptr(start_routine, 0) {
        return Err(SysErrCode::AddrNotValid);
    }
    let current = tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?;

    let mut fn_args = Args::default();
    *fn_args.get_mut(0) = Arg::from_ptr(args as *mut ());

    let task = unsafe { TaskBuilder::from_addr(VirtAddr::from_ptr(start_routine)) }
        .map_err(|_| SysErrCode::AddrNotValid)?
        .like_existing_usr(&current)
        .map_err(|_| SysErrCode::BadMsg)?
        .with_args(fn_args)
        .build();
    let tid = task.tid().get_inner();
    add_built_task(task);
    Ok(tid)
}

pub fn thread_exit() -> ! {
    exit(0)
}

pub fn thread_cancel(id: u64) -> SysCallRes<i64> {
    let r = tls::task_data()
        .kill(&id.into(), 0)
        .map(|_| 0)
        .ok_or(SysErrCode::NoProcess);
    post_event(WaitEvent::with_data(
        QueueType::Thread(id.into()),
        TaskStateChange::EXIT.bits() as u64,
    ));
    r
}

pub fn thread_join(
    id: u64,
    timeout: i64,
    w_flags: WaitOptions,
    tw_flags: TaskWaitOptions,
) -> SysCallRes<TaskStateChange> {
    let task = tls::task_data()
        .thread(&id.into())
        .ok_or(SysErrCode::NoChild)?;
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
        return Err(SysErrCode::WouldBlock);
    }
    let q_type = QueueType::Thread(id.into());
    add_queue(
        QueueHandle::from_owned(Box::new(GenericWaitQueue::new()) as Box<dyn WaitQueue>),
        q_type.clone(),
    );

    let r = wait_self(&conditions)
        .ok_or(SysErrCode::NoProcess)
        .map(|_| match task.state() {
            TaskState::Running | TaskState::Ready => TaskStateChange::WAKEUP,
            TaskState::Blocking | TaskState::Sleeping => TaskStateChange::BLOCK,
            TaskState::Zombie => TaskStateChange::EXIT,
        });
    remove_queue(&q_type);
    r
}

pub fn get_tid() -> SysCallRes<u64> {
    tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)
        .map(|t| t.tid().get_inner())
}

pub fn time() -> SysCallRes<u64> {
    // TODO this should return a u128, but this requires splitting across registers / ptr
    Ok(current_time().as_millis() as u64)
}

pub fn get_pgrid() -> SysCallRes<u64> {
    tls::task_data()
        .current_thread()
        .map(|p| p.pgrid().0)
        .ok_or(SysErrCode::NoProcess)
}

pub fn pipe(fds: *mut [u32; 2], cap: isize) -> SysCallRes<()> {
    if !valid_ptr(fds, 1) {
        return Err(SysErrCode::AddrNotValid);
    }
    let current_task = tls::task_data()
        .current_thread()
        .ok_or(SysErrCode::NoProcess)?;
    let pipe = Arc::new(Pipe::new(cap));

    let reader = FileBuilder::new(pipe.clone() as Arc<dyn FileRepr>)
        .with_perms(FPerms::READ)
        .finish();
    let writer = FileBuilder::new(pipe as Arc<dyn FileRepr>)
        .with_perms(FPerms::WRITE)
        .finish();

    let read_fd = current_task.next_fd();
    current_task.add_fd(read_fd, reader);

    let write_fd = current_task.next_fd();
    current_task.add_fd(write_fd, writer);

    let arr = unsafe { &mut *fds };
    arr[0] = read_fd;
    arr[1] = write_fd;
    Ok(())
}
