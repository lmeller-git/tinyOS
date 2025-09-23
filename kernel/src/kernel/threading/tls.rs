use alloc::collections::{btree_map::BTreeMap, vec_deque::VecDeque};
use core::{
    fmt::Debug,
    sync::atomic::{AtomicU64, Ordering},
};

use conquer_once::spin::OnceCell;

use crate::{
    kernel::threading::{
        schedule::{GlobalTaskPtr, Scheduler},
        task::{ExitInfo, TaskID, TaskRepr, TaskState, TaskStateData},
    },
    sync::locks::{Mutex, RwLock},
};

static GLOBAL_TASK_MANAGER: OnceCell<TaskManager> = OnceCell::uninit();

#[derive(Debug)]
pub struct TaskManager {
    tasks: RwLock<BTreeMap<TaskID, GlobalTaskPtr>>,
    current_running: AtomicU64, // TaskID
    zombies: Mutex<VecDeque<TaskID>>,
}

impl TaskManager {
    fn new() -> Self {
        Self {
            tasks: RwLock::new(BTreeMap::new()),
            current_running: TaskID::default().get_inner().into(),
            zombies: Mutex::new(VecDeque::new()),
        }
    }

    pub fn get(&self, task: &TaskID) -> Option<GlobalTaskPtr> {
        self.tasks.read().get(task).cloned()
    }

    pub fn get_current(&self) -> Option<GlobalTaskPtr> {
        self.get(&self.current_pid())
    }

    pub fn try_get(&self, task: &TaskID) -> Option<GlobalTaskPtr> {
        self.tasks.try_read()?.get(task).cloned()
    }

    pub fn try_get_current(&self) -> Option<GlobalTaskPtr> {
        self.try_get(&self.current_pid())
    }

    pub fn current_pid(&self) -> TaskID {
        (&self.current_running).into()
    }

    pub fn update_current(&self, task: TaskID) {
        self.current_running
            .store(task.get_inner(), Ordering::Release);
    }

    pub fn add(&self, task: GlobalTaskPtr) -> Option<GlobalTaskPtr> {
        let pid = task.pid();
        self.tasks.write().insert(pid, task)
    }

    pub fn try_add(&self, task: GlobalTaskPtr) -> Option<GlobalTaskPtr> {
        let pid = task.pid();
        self.tasks.try_write()?.insert(pid, task)
    }

    pub fn cleanup(&self) {
        let tasks = self.tasks.read();
        for (id, task) in tasks.iter() {
            if task.state() == TaskState::Zombie {
                self.zombies.lock().push_back(*id);
            }
        }

        drop(tasks);
        // cleanup zombies and remove them from self.tasks
        while let Some(zombie) = self.zombies.lock().pop_front() {
            let Some(task) = self.tasks.write().remove(&zombie) else {
                continue;
            };
            cleanup_task(task);
        }
    }

    pub fn update(&self, task: &GlobalTaskPtr) {
        match task.state() {
            TaskState::Zombie => {
                self.zombies.lock().push_back(task.pid());
            }
            _ => _ = self.add(task.clone()),
        }
    }

    pub fn try_update(&self, task: &GlobalTaskPtr) -> Option<()> {
        match task.state() {
            TaskState::Zombie => {
                self.zombies.try_lock()?.push_back(task.pid());
            }
            _ => _ = self.try_add(task.clone())?,
        }
        Some(())
    }

    pub fn get_table(&self) -> &RwLock<BTreeMap<TaskID, GlobalTaskPtr>> {
        &self.tasks
    }

    pub fn kill(&self, id: &TaskID, signal: i32) -> Option<()> {
        let task = self.get(id)?;
        task.set_state(TaskState::Zombie);
        *task.state_data().lock() = TaskStateData::Exit(ExitInfo {
            exit_code: signal as u32,
            signal: None,
        });
        self.update(&task);
        Some(())
    }

    pub fn block(&self, id: &TaskID) -> Option<()> {
        let task = self.get(id)?;
        if task.state() != TaskState::Zombie && task.state() != TaskState::Sleeping {
            task.set_state(TaskState::Blocking);
            Some(())
        } else {
            None
        }
    }

    pub fn try_block(&self, id: &TaskID) -> Option<()> {
        let task = self.try_get(id)?;
        if task.state() != TaskState::Zombie && task.state() != TaskState::Sleeping {
            task.set_state(TaskState::Blocking);
            Some(())
        } else {
            None
        }
    }

    pub fn try_wake(&self, id: &TaskID) -> Option<()> {
        let task = self.try_get(id)?;
        if task.state() == TaskState::Blocking || task.state() == TaskState::Sleeping {
            task.set_state(TaskState::Ready);
        }
        Some(())
    }

    pub fn wake(&self, id: &TaskID) -> Option<()> {
        let task = self.get(id)?;
        if task.state() == TaskState::Blocking || task.state() == TaskState::Sleeping {
            task.set_state(TaskState::Ready);
        }
        Some(())
    }
}

pub fn task_data<'a>() -> &'a TaskManager {
    GLOBAL_TASK_MANAGER.get_or_init(TaskManager::new)
}

fn cleanup_task(task: GlobalTaskPtr) {
    // TODO
}
