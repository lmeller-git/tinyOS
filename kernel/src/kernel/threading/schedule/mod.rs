use super::{
    ProcessEntry, ThreadingError,
    task::{SimpleTask, Task, TaskBuilder, TaskID, TaskPtr, TaskRepr},
};
use crate::{
    arch::{
        self,
        context::{TaskCtx, TaskState, switch_and_apply},
        mem::VirtAddr,
    },
    kernel::threading::task::Uninit,
    locks::thread_safe::{Mutex, MutexGuard, RwLock},
    serial_println,
};
use alloc::{string::String, sync::Arc};
use conquer_once::spin::OnceCell;

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
    fn wake(&mut self, id: &TaskID);
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

//SAFETY This function is EXTREMELY unsafe and the caller must ensure that no other function runs in parallel to this one, as well as keeping the state of sched consistent
pub unsafe fn with_scheduler_unckecked<F, R>(f: F) -> R
where
    F: FnOnce(&mut GlobalScheduler) -> R,
{
    use crate::arch::interrupt::without_interrupts;
    without_interrupts(|| {
        let mut was_locked = false;
        let mut sched = if let Ok(s) = GLOBAL_SCHEDULER.get_unchecked().try_lock() {
            s
        } else {
            was_locked = true;
            GLOBAL_SCHEDULER.get_unchecked().force_unlock();
            GLOBAL_SCHEDULER.get_unchecked().lock()
        };
        let res = f(&mut *sched);
        drop(sched);
        if was_locked {
            GLOBAL_SCHEDULER.get_unchecked().force_lock();
        }
        res
    })
}

pub fn with_current_task<F, R>(f: F) -> Option<R>
where
    F: FnOnce(GlobalTaskPtr) -> R,
{
    let guard = get()?;
    let task = guard.current()?;
    Some(f(task))
}

pub fn current_task() -> Result<GlobalTaskPtr, ThreadingError> {
    let guard = get().ok_or(ThreadingError::Unknown("could not get schduler".into()))?;
    let task = guard.current().ok_or(ThreadingError::Unknown(
        "no task runnign but requested".into(),
    ))?;
    Ok(task.clone())
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
        // let task: *const GlobalTask = &*guard as *const _;
        let task = TaskState::from_task(&*guard);
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
