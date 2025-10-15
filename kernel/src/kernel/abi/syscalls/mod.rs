use crate::{
    arch::context::SysCallCtx,
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
mod sys_core;
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
            OpenOptions::from_bits(args.third() as u32),
        )
        .map(|r| r as i64),
        SysCallDispatch::Close => close(args.first() as u32).map(|_| 0),
        SysCallDispatch::Read => read(
            args.first() as u32,
            args.second() as usize as *mut u8,
            args.third() as usize,
            args.fourth() as u64,
        )
        .map(|r| r as i64),
        SysCallDispatch::Write => write(
            args.first() as u32,
            args.second() as usize as *const u8,
            args.fourth() as usize,
        )
        .map(|r| r as i64),
        SysCallDispatch::Yield => yield_now().map(|_| 0),
        SysCallDispatch::Exit => exit(args.first()).max(|_| 0),
        SysCallDispatch::Kill => kill(args.first() as u64, args.second()).map(|_| 0),
        SysCallDispatch::Mmap => mmap(
            args.first() as usize,
            args.second() as usize as *mut u8,
            args.third() as u32,
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

// pub extern "C" fn syscall_handler(args: &mut SysCallCtx) {
//     serial_println!("syscall {} hit", args.rax);
//     let res: SysRetCode = match args.num() {
//         1 => {
//             sys_exit(args.first() as i64);
//             unreachable!()
//         }
//         2 => sys_kill(args.first(), args.second() as i64),
//         3 => sys_yield(),
//         4 => {
//             let written = sys_write(
//                 args.first() as usize,
//                 args.second() as *const u8,
//                 args.third() as usize,
//             );
//             args.ret2(written as i64);
//             if written >= 0 {
//                 SysRetCode::Success
//             } else {
//                 SysRetCode::Fail
//             }
//         }
//         5 => {
//             let written = sys_write_single(
//                 args.first() as usize,
//                 args.second(),
//                 args.third() as *const u8,
//                 args.fourth() as usize,
//             );
//             args.ret2(written as i64);
//             if written >= 0 {
//                 SysRetCode::Success
//             } else {
//                 SysRetCode::Fail
//             }
//         }
//         6 => {
//             let n_read = sys_read(
//                 args.first() as usize,
//                 args.second() as *mut u8,
//                 args.third() as usize,
//                 args.fourth() as usize,
//             );
//             args.ret2(n_read as i64);
//             if n_read < 0 {
//                 SysRetCode::Fail
//             } else {
//                 SysRetCode::Success
//             }
//         }
//         7 => {
//             let r = sys_heap(args.first() as usize);
//             args.ret2(r as i64);
//             if r.is_null() {
//                 SysRetCode::Fail
//             } else {
//                 SysRetCode::Success
//             }
//         }
//         8 => match sys_map_device(args.first() as *mut ()) {
//             Err(_) => SysRetCode::Fail,
//             Ok(addr) => {
//                 args.ret2(addr.0 as usize as i64);
//                 args.ret(addr.1 as u64 as i64);
//                 return;
//             }
//         },
//         9 => {
//             sys_gfx_config(args.first() as *mut GFXConfig);
//             SysRetCode::Success
//         }
//         10 => SysRetCode::Success, // No action
//         11 => {
//             sys_shutdown();
//             unreachable!()
//         }
//         _ => SysRetCode::Unknown,
//     };

//     args.ret(res as i64);
// }

#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysRetCode {
    Unknown = -2,
    Success = 0,
    Fail = -1,
}
