use alloc::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    vec::Vec,
};
use core::{
    fmt::Debug,
    sync::atomic::{AtomicU64, Ordering},
};

use conquer_once::spin::OnceCell;
use hashbrown::HashMap;

use crate::{
    kernel::threading::{
        schedule::{GlobalTaskPtr, Scheduler},
        task::{ExitInfo, ProcessGroupID, ProcessID, TaskID, TaskRepr, TaskState, TaskStateData},
    },
    sync::locks::{Mutex, RwLock},
};

static GLOBAL_TASK_MANAGER: OnceCell<TaskManager> = OnceCell::uninit();

// TODO:
// we want process groupes -> process -> threads
// a single session is assumed, thus we do not provide session api
// one process group is foreground and connected to the controlling tty
// the ID of a thread is ProcessGroupID - ProcessID - ThreadID
// all threads are cleaned up once a process exits
// all members of a process group are sent HUP + CONT if it becomes orphaned (ie it leader exits)
// TODO this also requires an implementation of signals + signal hooks
// TODO we also need some form of a controlling tty
// --> could store in a HashMap of Hash(PGID, PID, TID), with current being the hash
// Either:
// store only Hash + (intrusive?) tree of processes and walk on query
// or:
// store Hashmap and build process data on the fly if queried / store in additional tree

#[derive(Default, Debug)]
struct ProcessGroup {
    members: BTreeMap<ProcessID, Process>,
    leader: Option<ProcessID>,
}

impl ProcessGroup {
    fn new(id: ProcessID, leader: Process) -> Self {
        let mut members = BTreeMap::new();
        members.insert(id, leader);
        Self {
            members,
            leader: Some(id),
        }
    }

    fn add(&mut self, id: ProcessID, process: Process) -> Option<Process> {
        self.members.insert(id, process)
    }
}

#[derive(Default, Debug)]
struct Process {
    threads: BTreeMap<TaskID, GlobalTaskPtr>,
}

impl Process {
    fn new(leader: GlobalTaskPtr) -> Self {
        let mut threads = BTreeMap::new();
        threads.insert(leader.tid(), leader);
        Self { threads }
    }
}

#[derive(Debug)]
pub struct TaskManager {
    current_running: AtomicU64, // TaskID of the curently active thread
    lut: RwLock<HashMap<TaskID, GlobalTaskPtr>>, // LUT for thread id --> thread
    tree: RwLock<BTreeMap<ProcessGroupID, ProcessGroup>>,
    zombies: Mutex<VecDeque<TaskID>>,
}

impl TaskManager {
    fn new() -> Self {
        Self {
            current_running: TaskID::default().get_inner().into(),
            lut: RwLock::default(),
            tree: RwLock::default(),
            zombies: Mutex::default(),
        }
    }

    pub fn get(&self, task: &TaskID) -> Option<GlobalTaskPtr> {
        self.lut.read().get(task).cloned()
    }

    pub fn get_current(&self) -> Option<GlobalTaskPtr> {
        self.get(&self.current_pid())
    }

    pub fn try_get(&self, task: &TaskID) -> Option<GlobalTaskPtr> {
        self.lut.try_read()?.get(task).cloned()
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
        _ = self
            .tree
            .write()
            .entry(task.pgrid())
            .and_modify(|entry| {
                _ = entry.add(pid, Process::new(task.clone()));
            })
            .or_insert(ProcessGroup::new(pid, Process::new(task.clone())));

        self.lut.write().insert(task.tid(), task)
    }

    pub fn cleanup(&self) {
        let tasks = self.lut.read();
        for (id, task) in tasks.iter() {
            if task.state() == TaskState::Zombie {
                self.zombies.lock().push_back(*id);
            }
        }

        drop(tasks);
        // cleanup zombies and remove them from self.tasks
        while let Some(zombie) = self.zombies.lock().pop_front() {
            let Some(task) = self.lut.write().remove(&zombie) else {
                continue;
            };
            cleanup_task(task);
        }
    }

    pub fn update(&self, task: &GlobalTaskPtr) {
        match task.state() {
            TaskState::Zombie => {
                self.zombies.lock().push_back(task.tid());
            }
            _ => _ = self.add(task.clone()),
        }
    }

    pub fn get_table(&self) -> &RwLock<HashMap<TaskID, GlobalTaskPtr>> {
        &self.lut
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
