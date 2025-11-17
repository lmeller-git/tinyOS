use alloc::{string::String, sync::Arc};

use conquer_once::spin::OnceCell;

use super::{
    ProcessEntry,
    ThreadingError,
    task::{TaskBuilder, TaskRepr, ThreadID},
};
use crate::{
    arch::{
        context::{TaskState, switch_and_apply},
        interrupt::gdt::set_tss_kstack,
        mem::VirtAddr,
    },
    kernel::threading::{
        task::{Task, Uninit},
        tls,
    },
    serial_println,
};

mod round_robin;

pub trait Scheduler {
    fn new() -> Self;
    fn reschedule(&self);
    fn switch(&self) -> Option<ThreadID>;
    fn add_task(&self, id: ThreadID);
}

pub enum ScheduleOrder {}

pub type GlobalScheduler = round_robin::LazyRoundRobin;
pub type GlobalTask = Task;
pub type GlobalTaskPtr = Arc<GlobalTask>;

static GLOBAL_SCHEDULER: OnceCell<GlobalScheduler> = OnceCell::uninit();

pub fn init() {
    _ = GLOBAL_SCHEDULER.try_init_once(GlobalScheduler::new);
}

pub fn with_scheduler<F, R>(f: F) -> R
where
    F: FnOnce(&GlobalScheduler) -> R,
{
    let s = get_scheduler();
    crate::arch::interrupt::without_interrupts(|| f(s))
}

pub fn current_pid() -> u64 {
    tls::task_data().current_tid().get_inner()
}

pub fn with_current_task<F, R>(f: F) -> Option<R>
where
    F: FnOnce(GlobalTaskPtr) -> R,
{
    let task = current_task().ok()?;
    Some(f(task))
}

pub fn get_scheduler<'a>() -> &'a GlobalScheduler {
    GLOBAL_SCHEDULER.get_or_init(GlobalScheduler::new)
}

#[allow(static_mut_refs)]
pub fn current_task() -> Result<GlobalTaskPtr, ThreadingError> {
    tls::task_data()
        .get_current()
        .ok_or(ThreadingError::Unknown(
            "could not find current task".into(),
        ))
}

#[allow(unsafe_op_in_unsafe_fn, dropping_references, dropping_copy_types)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn context_switch_local(rsp: u64) {
    // heart of context switching logic. Here we get the next task to run, initialize task_data and scheduler and switch.
    // WE CANNOT BLOCK HERE

    let task_data = tls::task_data();
    let current = if let Some(current) = task_data.try_get_current() {
        current.set_krsp(&VirtAddr::new(rsp));
        current
    } else if task_data.current_tid() == ThreadID::default() {
        let Some(current) = task_data.get(&1.into()) else {
            serial_println!("{:#?}", task_data);
            panic!("could not load initial task");
        };
        current
    } else {
        return;
    };

    let Some(next) = get_scheduler().switch() else {
        return;
    };
    let Some(next_task) = task_data.try_get(&next) else {
        todo!()
    };
    if current.state() == super::task::TaskState::Running {
        current.set_state(super::task::TaskState::Ready);
    }
    task_data.update_current(next);

    let ptr = TaskState::from_task(next_task.as_ref());

    set_tss_kstack(*next_task.kstack_top());

    drop(next_task);
    drop(next);
    drop(current);
    drop(task_data);

    switch_and_apply(ptr);
    unreachable!()
}

pub fn add_task_ptr__(ptr: GlobalTaskPtr) {
    get_scheduler().add_task(ptr.tid());
    tls::task_data().add(ptr);
}

pub fn add_built_task(task: GlobalTask) {
    add_task_ptr__(task.into());
}

pub fn add_named_ktask(func: ProcessEntry, name: String) -> Result<(), ThreadingError> {
    let task = TaskBuilder::from_fn(func)?
        .with_name(name)
        .with_default_files(true)
        .as_kernel()?
        .build();
    add_built_task(task);
    Ok(())
}

pub fn add_ktask(func: ProcessEntry) -> Result<(), ThreadingError> {
    let task = TaskBuilder::from_fn(func)?
        .with_default_files(true)
        .as_kernel()?
        .build();
    add_built_task(task);
    Ok(())
}

pub fn add_named_usr_task(func: ProcessEntry, name: String) -> Result<(), ThreadingError> {
    let task = TaskBuilder::from_fn(func)?
        .with_name(name)
        .with_default_files(true);
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
    let task: TaskBuilder<Task, crate::kernel::threading::task::Init> =
        TaskBuilder::<Task, Uninit>::from_addr(addr)?;
    let task = task.with_name(name).with_default_files(true);
    let task = task.as_usr()?;
    let task = task.build();
    add_built_task(task);
    Ok(())
}
