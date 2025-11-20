use tinyos_abi::{
    consts::MAX_SYSCALL,
    flags::{OpenOptions, PageTableFlags, TaskWaitOptions, WaitOptions},
    types::{SysCallDispatch, SysRetCode},
};

use crate::{
    arch::context::SysCallCtx,
    eprintln,
    kernel::abi::syscalls::funcs::{
        close,
        dup,
        eventfd,
        execve,
        exit,
        fork,
        get_pgrid,
        get_pid,
        get_tid,
        kill,
        mmap,
        munmap,
        open,
        pipe,
        read,
        seek,
        serial,
        spawn,
        thread_cancel,
        thread_create,
        thread_exit,
        thread_join,
        time,
        wait_pid,
        waittime,
        write,
        yield_now,
    },
    println,
    serial_println,
};

pub mod funcs;
pub mod utils;

// all syscalls return their first return value in rax (x86_64) and their error value in rdx (x86_64)

pub extern "C" fn syscall_handler(args: &mut SysCallCtx) {
    let dispatch = args.num();
    if dispatch > MAX_SYSCALL {
        eprintln!(
            "tried to call a syscall with an invalid number: {}. Only 0..{} are valid.",
            args.num(),
            MAX_SYSCALL
        );
        args.ret(SysRetCode::Fail as i64);
        return;
    }
    let dispatch = unsafe { core::mem::transmute(dispatch) };

    let res = match dispatch {
        SysCallDispatch::Open => open(
            args.first() as usize as *const u8,
            args.second() as usize,
            OpenOptions::from_bits_truncate(args.third() as u32),
        )
        .map(|r| r as i64),
        SysCallDispatch::Close => close(args.first() as u32).map(|_| 0),
        SysCallDispatch::Read => read(
            args.first() as u32,
            args.second() as usize as *mut u8,
            args.third() as usize,
            args.fourth() as i64,
        )
        .map(|r| r as i64),
        SysCallDispatch::Write => write(
            args.first() as u32,
            args.second() as usize as *const u8,
            args.third() as usize,
        )
        .map(|r| r as i64),
        SysCallDispatch::Yield => yield_now().map(|_| 0),
        SysCallDispatch::Exit => exit(args.first() as i64),
        SysCallDispatch::Kill => kill(args.first(), args.second() as i64).map(|_| 0),
        SysCallDispatch::Mmap => mmap(
            args.first() as usize,
            args.second() as usize as *mut u8,
            PageTableFlags::from_bits_truncate(args.third()),
            args.fourth() as i32,
        )
        .map(|r| r as usize as i64),
        SysCallDispatch::Munmap => {
            munmap(args.first() as usize as *mut u8, args.second() as usize).map(|_| 0)
        }
        SysCallDispatch::Fork => fork().map(|r| r as i64),
        SysCallDispatch::WaitTime => waittime(args.first()).map(|_| 0),
        SysCallDispatch::GetPID => get_pid().map(|r| r as i64),
        SysCallDispatch::Seek => seek(args.first() as u32, args.second() as usize).map(|_| 0),
        SysCallDispatch::Dup => dup(args.first() as u32, args.second() as i32).map(|r| r as i64),
        SysCallDispatch::Spawn => {
            spawn(args.first() as *const u8, args.second() as usize).map(|_| 0)
        }
        SysCallDispatch::Dbg => {
            serial(args.first() as *const u8, args.second() as usize).map(|_| 0)
        }
        SysCallDispatch::Execve => {
            execve(args.first() as *const u8, args.second() as usize).map(|r| r as i64)
        }
        SysCallDispatch::ThreadCreate => {
            thread_create(args.first() as *const (), args.second() as *const ()).map(|r| r as i64)
        }
        SysCallDispatch::ThreadExit => thread_exit(),
        SysCallDispatch::ThreadCancel => thread_cancel(args.first()),
        SysCallDispatch::ThreadJoin => thread_join(
            args.first(),
            args.second() as i64,
            WaitOptions::from_bits_truncate(args.third() as u16),
            TaskWaitOptions::from_bits_truncate(args.fourth() as u16),
        )
        .map(|r| r.bits() as i64),
        SysCallDispatch::WaitPID => wait_pid(
            args.first(),
            args.second() as i64,
            WaitOptions::from_bits_truncate(args.third() as u16),
            TaskWaitOptions::from_bits_truncate(args.fourth() as u16),
        )
        .map(|r| r.bits() as i64),
        SysCallDispatch::EventFD => eventfd().map(|r| r as i64),
        SysCallDispatch::Time => time().map(|r| r as i64),
        SysCallDispatch::GetTID => get_tid().map(|r| r as i64),
        SysCallDispatch::GetPgrID => get_pgrid().map(|r| r as i64),
        SysCallDispatch::Pipe => pipe(args.first() as *mut [u32; 2]).map(|_| 0),
    };

    // in case of err we return the error value in ret2 and do not touch ret1
    // in case of success we return the return value in ret1 and return success value in ret2
    res.inspect_err(|e| args.ret2(*e as i64)).inspect(|r| {
        args.ret(*r);
        args.ret2(SysRetCode::Success as i64);
    });
}
