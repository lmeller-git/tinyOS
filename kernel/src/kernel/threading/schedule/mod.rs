use super::{
    ProcessEntry, ThreadingError,
    task::{SimpleTask, Task, TaskBuilder, TaskID, TaskPtr, TaskRepr},
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
    fn add_task(&mut self, task: GlobalTaskPtr);
    fn yield_now(&mut self);
    fn cleanup(&mut self);
    fn kill(&mut self, id: TaskID);
    fn switch(&mut self) -> Option<GlobalTaskPtr>;
    fn init(&mut self);
    fn current(&self) -> Option<GlobalTaskPtr>;
    fn num_tasks(&self) -> usize;
    fn reschedule(&mut self, order: ScheduleOrder);
    fn current_mut(&mut self) -> &mut Option<GlobalTaskPtr>;
}

pub enum ScheduleOrder {}

pub type GlobalScheduler = round_robin::OneOneRoundRobin;
pub type GlobalTask = SimpleTask;
pub type TaskPtr_<T: TaskRepr> = Arc<RwLock<T>>;
pub type GlobalTaskPtr = TaskPtr<GlobalTask>;

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
    F: FnOnce(GlobalTaskPtr) -> R,
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
        // #[cfg(not(feature = "test_run"))]
        // serial_println!(
        //     "old krsp: {:#x}, new krsp: {:#x}",
        //     current.read_inner().krsp,
        //     rsp
        // );
        current.write_inner().krsp = VirtAddr::new(rsp);
    }
    if let Some(new) = lock.switch() {
        // #[cfg(not(feature = "test_run"))]
        // serial_println!("new task, {:#?}", new);
        unsafe { GLOBAL_SCHEDULER.get_unchecked().force_unlock() };
        let guard = new.raw().read();
        let task: *const GlobalTask = &*guard as *const _;
        drop(guard);
        switch_and_apply(task);
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

pub fn add_task_ptr__(ptr: GlobalTaskPtr) {
    unsafe { get_unchecked() }.add_task(ptr);
}

pub fn add_built_task(task: GlobalTask) {
    add_task_ptr__(task.into());
}

pub fn add_named_ktask(func: ProcessEntry, name: String) -> Result<(), ThreadingError> {
    #[cfg(not(feature = "test_run"))]
    serial_println!("spawning task {} at {:#x}", name, func as usize);
    let task = TaskBuilder::from_fn(func)?
        .with_name(name)
        .as_kernel()?
        .build();
    serial_println!("task built");
    add_built_task(task);
    Ok(())
}

pub fn add_ktask(func: ProcessEntry) -> Result<(), ThreadingError> {
    serial_println!("spawning task {:#x}", func as usize);
    let task = TaskBuilder::from_fn(func)?.as_kernel()?.build();
    serial_println!("task built");
    add_built_task(task);
    Ok(())
}

pub fn add_named_usr_task(func: ProcessEntry, name: String) -> Result<(), ThreadingError> {
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
