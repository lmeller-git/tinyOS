use core::{
    hint::unreachable_unchecked,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    ProcessEntry, ThreadingError,
    task::{SimpleTask, TaskBuilder, TaskID, TaskPtr, TaskRepr},
};
use crate::{
    arch::{
        self,
        context::{TaskCtx, TaskState, switch_and_apply},
        interrupt::gdt::set_tss_kstack,
        mem::VirtAddr,
    },
    kernel::threading::task::Uninit,
    locks::{
        GKL,
        reentrant::{Mutex, MutexGuard, RwLock},
    },
    serial_println,
};
use alloc::{string::String, sync::Arc};
use conquer_once::spin::OnceCell;

#[cfg(feature = "test_run")]
pub mod testing;

mod round_robin;

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

static GLOBAL_SCHEDULER: OnceCell<Mutex<GlobalScheduler>> = OnceCell::uninit();

static CURRENT_PID: AtomicU64 = AtomicU64::new(0);
#[allow(static_mut_refs)]
static mut CURRENT_TASK: Option<GlobalTaskPtr> = None;

pub fn init() {
    _ = GLOBAL_SCHEDULER.try_init_once(|| Mutex::new(GlobalScheduler::new()));
}

pub fn with_scheduler<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&Mutex<GlobalScheduler>) -> R,
{
    let s = GLOBAL_SCHEDULER.get()?;
    Some(crate::arch::interrupt::without_interrupts(|| f(s)))
}

pub fn current_pid() -> u64 {
    CURRENT_PID.load(Ordering::Acquire)
}

pub unsafe fn set_current_pid(v: u64) {
    CURRENT_PID.store(v, Ordering::Release);
}

#[allow(static_mut_refs)]
pub fn set_current_task(task: GlobalTaskPtr) {
    unsafe { CURRENT_TASK.replace(task) };
}

pub fn with_current_task<F, R>(f: F) -> Option<R>
where
    F: FnOnce(GlobalTaskPtr) -> R,
{
    let task = current_task().ok()?;
    Some(f(task))
}

#[allow(static_mut_refs)]
pub fn current_task() -> Result<GlobalTaskPtr, ThreadingError> {
    unsafe { CURRENT_TASK.clone() }.ok_or(ThreadingError::Unknown("no task registered".into()))
}

#[allow(unsafe_op_in_unsafe_fn)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn context_switch_local(rsp: u64) {
    // TODO currently this simply returns if a) scheduler is locked or b) current task is locked
    // this is a bad thing
    //
    // further allocations during context_switch/ in interrupt free ctx may lead to deadlocks/double faults
    // need to fix/ find workaround

    // GKL (in particular sched + the current running task) need to be completely unlocked, as even reentrancy would lead to deadlock
    // (task n holds lock to itself -> switch -> goes through due to reentrancy -> task n+1 runs -> switch -> we try to lock. But does not work as lock is still held by task n -> repeat)
    #[cfg(feature = "gkl")]
    if GKL.is_locked() {
        return;
    }

    if let Ok(mut lock) = GLOBAL_SCHEDULER.get().unwrap().try_lock() {
        if let Some(current) = lock.current_mut() {
            // let Ok(mut current) = current.raw().try_write() else {
            //     serial_println!("current locked");
            //     return;
            // };
            let current = current.inner_unchecked();
            current.krsp = VirtAddr::new(rsp);
        }
        if let Some(new) = lock.switch() {
            // let Ok(guard) = new.raw().try_read() else {
            //     serial_println!("task we wanted to switch to is locked");
            //     return;
            // };
            let guard = new.inner_unchecked();
            let task = TaskState::from_task(guard);
            set_current_pid(guard.pid.get_inner());
            #[allow(dropping_references)]
            drop(guard);
            set_current_task(new);
            drop(lock);
            set_tss_kstack(VirtAddr::new(task.rsp));
            switch_and_apply(task);
            unreachable!()
        }
    }
    #[cfg(not(feature = "gkl"))]
    serial_println!("sched locked");
}

#[allow(unsafe_op_in_unsafe_fn)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn context_switch(
    state: arch::context::ReducedCpuInfo,
    frame: arch::interrupt::handlers::InterruptStackFrame,
) {
}

pub fn add_task_ptr__(ptr: GlobalTaskPtr) {
    with_scheduler(|sched| sched.lock().add_task(ptr));
}

pub fn add_built_task(task: GlobalTask) {
    add_task_ptr__(task.into());
}

pub fn add_named_ktask(func: ProcessEntry, name: String) -> Result<(), ThreadingError> {
    let task = TaskBuilder::from_fn(func)?
        .with_name(name)
        .with_default_devices()
        .as_kernel()?
        .build();
    add_built_task(task);
    Ok(())
}

pub fn add_ktask(func: ProcessEntry) -> Result<(), ThreadingError> {
    let task = TaskBuilder::from_fn(func)?
        .with_default_devices()
        .as_kernel()?
        .build();
    add_built_task(task);
    Ok(())
}

pub fn add_named_usr_task(func: ProcessEntry, name: String) -> Result<(), ThreadingError> {
    let task = TaskBuilder::from_fn(func)?
        .with_name(name)
        .with_default_devices();
    let task = task.as_usr()?;
    let task = task.build();
    add_built_task(task);
    Ok(())
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn add_named_usr_task_from_addr(
    addr: VirtAddr,
    name: String,
) -> Result<(), ThreadingError> {
    let task: TaskBuilder<SimpleTask, crate::kernel::threading::task::Init> =
        TaskBuilder::<SimpleTask, Uninit>::from_addr(addr)?;
    let task = task.with_name(name).with_default_devices();
    let task = task.as_usr()?;
    let task = task.build();
    add_built_task(task);
    Ok(())
}
