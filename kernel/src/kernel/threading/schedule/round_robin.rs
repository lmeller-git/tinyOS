use alloc::collections::vec_deque::VecDeque;
use core::fmt::Debug;

use crate::{
    arch::interrupt,
    kernel::threading::{
        schedule::Scheduler,
        task::{ThreadID, TaskRepr, TaskState},
        tls,
    },
    serial_println,
    sync::{self, NoBlock},
};

#[derive(Debug)]
pub struct LazyRoundRobin {
    queue: sync::locks::GenericMutex<VecDeque<ThreadID>, NoBlock>,
}

impl LazyRoundRobin {
    pub fn log_all(&self) {
        serial_println!("LazyRoundRobin: tasks:");
        for t in self.queue.lock().iter() {
            serial_println!("{:?}", tls::task_data().get(t));
        }
    }
}

impl Scheduler for LazyRoundRobin {
    fn new() -> Self {
        Self {
            queue: sync::locks::GenericMutex::new(VecDeque::new()),
        }
    }

    fn reschedule(&self) {
        // TODO return if not dirty
        let manager = tls::task_data();

        let table = manager.get_table().read();
        let mut queue = self.queue.lock();

        interrupt::without_interrupts(|| {
            queue.clear();
            for (id, task) in table.iter() {
                if task.state() == TaskState::Ready || task.state() == TaskState::Running {
                    queue.push_back(task.tid());
                }
            }
        })
    }

    fn switch(&self) -> Option<ThreadID> {
        let mut queue = self.queue.try_lock()?;
        while let Some(id) = queue.pop_front() {
            let Some(task) = tls::task_data().try_get(&id) else {
                // Task was likely killed and removed from task manager
                continue;
            };
            if task.state() != TaskState::Ready {
                continue;
            }
            tls::task_data().update_current(id);
            queue.push_back(id);
            return Some(id);
        }
        None
    }

    fn add_task(&self, id: ThreadID) {
        self.queue.lock().push_back(id);
    }
}
