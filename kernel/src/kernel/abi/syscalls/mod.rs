use funcs::{sys_exit, sys_kill, sys_write, sys_write_single, sys_yield};

use crate::{arch::context::SysCallCtx, kernel::abi::syscalls::funcs::sys_read};

pub mod funcs;

pub extern "C" fn syscall_handler(args: &mut SysCallCtx) {
    let res: SysRetCode = match args.num() {
        1 => {
            sys_exit(args.first() as i64);
            unreachable!()
        }
        2 => sys_kill(args.first(), args.second() as i64),
        3 => sys_yield(),
        4 => {
            let written = sys_write(
                args.first() as usize,
                args.second() as *const u8,
                args.third() as usize,
            );
            args.ret2(written as i64);
            if written >= 0 {
                SysRetCode::Success
            } else {
                SysRetCode::Fail
            }
        }
        5 => {
            let written = sys_write_single(
                args.first() as usize,
                args.second(),
                args.third() as *const u8,
                args.fourth() as usize,
            );
            args.ret2(written as i64);
            if written >= 0 {
                SysRetCode::Success
            } else {
                SysRetCode::Fail
            }
        }
        6 => {
            let n_read = sys_read(
                args.first() as usize,
                args.second() as *mut u8,
                args.third() as usize,
            );
            args.ret2(n_read as i64);
            if n_read < 0 {
                SysRetCode::Fail
            } else {
                SysRetCode::Success
            }
        }
        _ => SysRetCode::Unknown,
    };

    args.ret(res as i64);
}

#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysRetCode {
    Unknown = -2,
    Success = 0,
    Fail = -1,
}
