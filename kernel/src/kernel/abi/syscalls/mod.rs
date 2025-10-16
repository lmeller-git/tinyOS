use crate::{
    arch::{context::SysCallCtx, mem::PageTableFlags},
    kernel::{
        abi::syscalls::funcs::{
            clone,
            close,
            exit,
            get_pid,
            kill,
            machine,
            mmap,
            munmap,
            open,
            read,
            wait,
            write,
            yield_now,
        },
        fs::OpenOptions,
    },
    serial_println,
};

pub mod funcs;
pub mod utils;

type SysCallRes<T> = Result<T, SysRetCode>;

#[repr(u64)]
enum SysCallDispatch {
    Open = 0,
    Close = 1,
    Read = 2,
    Write = 3,
    Yield = 4,
    Exit = 5,
    Kill = 6,
    Mmap = 7,
    Munmap = 8,
    Clone = 9,
    Wait = 10,
    Machine = 11,
    GetPid = 12,
}

const MAX_SYSCALL: u64 = 12;

pub extern "C" fn syscall_handler(args: &mut SysCallCtx) {
    let dispatch = args.num();
    if dispatch > MAX_SYSCALL {
        args.ret(SysRetCode::Fail as i64);
    }
    let dispatch = unsafe { core::mem::transmute(dispatch) };

    let res = match dispatch {
        SysCallDispatch::Open => open(
            args.first() as usize as *const u8,
            args.second() as usize,
            OpenOptions::from_bits(args.third() as u32).unwrap_or(OpenOptions::default()),
        )
        .map(|r| r as i64),
        SysCallDispatch::Close => close(args.first() as u32).map(|_| 0),
        SysCallDispatch::Read => read(
            args.first() as u32,
            args.second() as usize as *mut u8,
            args.third() as usize,
            args.fourth(),
        )
        .map(|r| r as i64),
        SysCallDispatch::Write => write(
            args.first() as u32,
            args.second() as usize as *const u8,
            args.fourth() as usize,
        )
        .map(|r| r as i64),
        SysCallDispatch::Yield => yield_now().map(|_| 0),
        SysCallDispatch::Exit => exit(args.first() as i64),
        SysCallDispatch::Kill => kill(args.first(), args.second() as i64).map(|_| 0),
        SysCallDispatch::Mmap => mmap(
            args.first() as usize,
            args.second() as usize as *mut u8,
            PageTableFlags::from_bits(args.third()).unwrap_or(PageTableFlags::empty()),
        )
        .map(|r| r as usize as i64),
        SysCallDispatch::Munmap => {
            munmap(args.first() as usize as *mut u8, args.second() as usize).map(|_| 0)
        }
        SysCallDispatch::Clone => clone().map(|r| r as i64),
        SysCallDispatch::Wait => wait().map(|_| 0),
        SysCallDispatch::Machine => machine().map(|_| 0),
        SysCallDispatch::GetPid => get_pid().map(|r| r as i64),
    };
    res.inspect_err(|e| args.ret(*e as i64))
        .inspect(|r| args.ret2(*r));
}

#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysRetCode {
    Unknown = -2,
    Success = 0,
    Fail = -1,
}
