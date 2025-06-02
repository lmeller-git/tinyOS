use super::{
    ThreadingError,
    task::{SimpleTask, Task, TaskBuilder, TaskID},
};
use crate::{
    arch::{
        self,
        context::{TaskCtx, switch_and_apply},
        mem::VirtAddr,
    },
    kernel::threading::task::Uninit,
    serial_println,
};
use alloc::{string::String, sync::Arc};
use conquer_once::spin::OnceCell;
use spin::{Mutex, MutexGuard, RwLock};

#[cfg(feature = "test_run")]
pub mod testing;

mod round_robin;

pub trait Scheduler {
    fn new() -> Self;
    fn add_task(&mut self, task: Task);
    fn yield_now(&mut self);
    fn cleanup(&mut self);
    fn kill(&mut self, id: TaskID);
    fn switch(&mut self, ctx: TaskCtx) -> Option<&TaskCtx>;
    fn init(&mut self);
    fn current(&self) -> Option<&Task>;
    fn num_tasks(&self) -> usize;
    fn reschedule(&mut self, order: ScheduleOrder);
}

pub trait OneOneScheduler {
    fn new() -> Self;
    fn add_task(&mut self, task: SimpleTask);
    fn yield_now(&mut self);
    fn cleanup(&mut self);
    fn kill(&mut self, id: TaskID);
    fn switch(&mut self) -> Option<&SimpleTask>;
    fn init(&mut self);
    fn current(&self) -> Option<&SimpleTask>;
    fn num_tasks(&self) -> usize;
    fn reschedule(&mut self, order: ScheduleOrder);
    fn current_mut(&mut self) -> &mut Option<SimpleTask>;
}

pub enum ScheduleOrder {}

pub type GlobalScheduler = round_robin::OneOneRoundRobin;
pub type GlobalTask = SimpleTask;
pub type GlobalTaskPtr = Arc<RwLock<GlobalTask>>;

pub static GLOBAL_SCHEDULER: OnceCell<Mutex<GlobalScheduler>> = OnceCell::uninit();

pub fn init() {
    _ = GLOBAL_SCHEDULER.try_init_once(|| Mutex::new(GlobalScheduler::new()));
}

pub fn get<'a>() -> Option<MutexGuard<'a, GlobalScheduler>> {
    GLOBAL_SCHEDULER.get().map(|s| s.lock())
}

pub unsafe fn get_unchecked<'a>() -> MutexGuard<'a, GlobalScheduler> {
    GLOBAL_SCHEDULER.get_unchecked().lock()
}

pub fn with_current_task<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&GlobalTask) -> R,
{
    let guard = get()?;
    let task = guard.current()?;
    Some(f(task))
}

#[allow(unsafe_op_in_unsafe_fn)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn context_switch_local(rsp: u64) {
    let mut lock = GLOBAL_SCHEDULER.get_unchecked().lock();
    if let Some(current) = lock.current_mut() {
        serial_println!("old krsp: {:#x}, new krsp: {:#x}", current.krsp, rsp);
        current.krsp = VirtAddr::new(rsp);
    }
    if let Some(new) = lock.switch() {
        serial_println!("new task, {:#?}", new);
        unsafe { GLOBAL_SCHEDULER.get_unchecked().force_unlock() };
        switch_and_apply(&new);
        unreachable!()
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn context_switch(
    state: arch::context::ReducedCpuInfo,
    frame: arch::interrupt::handlers::InterruptStackFrame,
) {
    // let ctx = TaskCtx::from_trap_ctx(frame, state);
    // if let Some(new) = GLOBAL_SCHEDULER.get_unchecked().lock().switch(ctx) {}
    // set_cpu_context(ctx);
}

pub fn add_built_task(task: SimpleTask) {
    unsafe { GLOBAL_SCHEDULER.get_unchecked() }
        .lock()
        .add_task(task);
}

pub fn add_named_ktask(func: extern "C" fn() -> usize, name: String) -> Result<(), ThreadingError> {
    serial_println!("spawning task {} at {:#x}", name, func as usize);
    let task = TaskBuilder::from_fn(func)?
        .with_name(name)
        .as_kernel()?
        .build();
    serial_println!("task built");
    add_built_task(task);
    Ok(())
}

pub fn add_ktask(func: extern "C" fn() -> usize) -> Result<(), ThreadingError> {
    serial_println!("spawning task {:#x}", func as usize);
    let task = TaskBuilder::from_fn(func)?.as_kernel()?.build();
    serial_println!("task built");
    add_built_task(task);
    Ok(())
}

pub fn add_named_usr_task(
    func: extern "C" fn() -> usize,
    name: String,
) -> Result<(), ThreadingError> {
    serial_println!("spawning user task {} at {:#x}", name, func as usize);
    let task = TaskBuilder::from_fn(func)?.with_name(name);
    serial_println!("task created");
    let task = task.as_usr()?;
    serial_println!("task setup");
    let task = task.build();
    serial_println!("task built");
    add_built_task(task);
    Ok(())
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn add_named_usr_task_from_addr(
    addr: VirtAddr,
    name: String,
) -> Result<(), ThreadingError> {
    serial_println!("spawning user task {} at {:#x}", name, addr);
    let task: TaskBuilder<SimpleTask, crate::kernel::threading::task::Init> =
        TaskBuilder::<SimpleTask, Uninit>::from_addr(addr)?;
    let task = task.with_name(name);
    serial_println!("task created");
    let task = task.as_usr()?;
    serial_println!("task setup");
    let task = task.build();
    serial_println!("task built");
    add_built_task(task);
    Ok(())
}
