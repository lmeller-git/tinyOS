use alloc::boxed::Box;
use core::{
    fmt::Debug,
    hash::{BuildHasher, BuildHasherDefault, Hash, Hasher},
    ops::Deref,
    ptr::NonNull,
};

use conquer_once::spin::OnceCell;
use hashbrown::HashMap;
use nblf_queue::{MPMCQueue, PooledStaticQueue, core::slots::TaggedPtr64};

use crate::{
    arch::interrupt,
    kernel::{
        fs::{Path, PathBuf},
        threading::{
            self,
            task::{ProcessID, ThreadID},
            tls,
            wait::{condition::WaitCondition, queues::WaitQueue},
        },
    },
    sync::locks::RwLock,
};

pub mod condition;
pub mod queues;

pub const MAX_WAIT_EVENTS: usize = 20;

pub static MESSAGE_QUEUE: OnceCell<
    PooledStaticQueue<WaitEvent<u64>, MAX_WAIT_EVENTS, TaggedPtr64>,
> = OnceCell::uninit();

pub fn init() {
    MESSAGE_QUEUE.init_once(|| PooledStaticQueue::with_slot());
}

pub fn post_event(event: WaitEvent<u64>) -> Result<(), WaitEvent<u64>> {
    MESSAGE_QUEUE
        .get_or_init(|| PooledStaticQueue::with_slot())
        .push(event)
}

pub fn get_event() -> Option<WaitEvent<u64>> {
    MESSAGE_QUEUE.get()?.pop()
}

pub(crate) struct QueueHandle<'a>(QueueHandleInner<'a>);

impl<'a> QueueHandle<'a> {
    pub fn from_owned(queue: Box<dyn WaitQueue>) -> Self {
        Self(QueueHandleInner::Owned(queue))
    }

    pub fn from_borrowed(queue: &'a dyn WaitQueue) -> Self {
        Self(QueueHandleInner::Borrowed(queue))
    }
}

enum QueueHandleInner<'a> {
    Borrowed(&'a dyn WaitQueue),
    Owned(Box<dyn WaitQueue>),
}

impl<'a> Deref for QueueHandle<'a> {
    type Target = dyn WaitQueue + 'a;

    fn deref(&self) -> &Self::Target {
        match &self.0 {
            QueueHandleInner::Borrowed(r) => *r,
            QueueHandleInner::Owned(b) => &**b,
        }
    }
}

pub struct WaitObserver<'a> {
    queues: RwLock<HashMap<QueueType, QueueHandle<'a>>>,
}

impl<'a> WaitObserver<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_queue(&mut self, queue: QueueHandle<'a>, queue_type: QueueType) -> Option<()> {
        self.queues
            .write()
            .try_insert(queue_type, queue)
            .ok()
            .map(|v| ())
    }

    pub fn remove_queue(&mut self, queue_type: &QueueType) {
        self.queues.write().remove_entry(queue_type);
    }

    pub fn process_signals(&self) {
        let map = self.queues.read();
        while let Some(s) = get_event() {
            map.get(&s.event_type).map(|q| q.signal());
        }
    }

    pub fn enqueue(&self, id: &ThreadID, queue_data: &[QueuTypeCondition]) {
        let map = self.queues.read();
        // TODO proper atomic multi enqueue
        for q in queue_data {
            if q.cond.is_given() {
                return;
            }
            map.get(&q.q_type)
                .map(|queue| queue.enqueue(id, q.cond.clone()));
        }
        drop(map);
        // at this point the task is enqueued, but not blocked.
        // we must now block it and ensure, that it van still be woken up, ie no condition is already true
        // retry n times. if this fails, the task is either already dead, or someone holds a writer lock

        for _ in 0..5 {
            if queue_data.iter().any(|c| c.cond.is_given()) {
                // do not block
                return;
            }
            // try to block and recheck condition
            if tls::task_data().try_block(id).is_some() {
                if queue_data.iter().any(|c| c.cond.is_given()) {
                    // need to unblock again, as we cannot ensured, that it got unblocked by queue
                    _ = tls::task_data().wake(id);
                }
                return;
            }
            threading::yield_now();
        }
    }
}

impl Default for WaitObserver<'_> {
    fn default() -> Self {
        Self {
            queues: RwLock::new(HashMap::new()),
        }
    }
}

// WaitObserver is Send + Sync,
// as all operations are atomic relative to observed task state
unsafe impl Send for WaitObserver<'_> {}
unsafe impl Sync for WaitObserver<'_> {}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum QueueType {
    Timer,
    KeyBoard,
    Thread(ThreadID),
    Process(ProcessID),
    File(u64),
    Lock(u64),
}

impl QueueType {
    pub fn file(path: &Path) -> Self {
        let mut hasher = hashbrown::DefaultHashBuilder::default().build_hasher();
        path.hash(&mut hasher);
        Self::File(hasher.finish())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct QueuTypeCondition {
    pub q_type: QueueType,
    pub cond: WaitCondition,
}

impl QueuTypeCondition {
    pub fn new(q_type: QueueType) -> Self {
        Self {
            q_type,
            cond: WaitCondition::None,
        }
    }

    pub fn with_cond(q_type: QueueType, cond: WaitCondition) -> Self {
        Self { q_type, cond }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct WaitEvent<D: Copy + Debug + PartialEq + Eq + Default + Send> {
    pub event_type: QueueType,
    pub data: D,
}

impl<D> WaitEvent<D>
where
    D: Copy + Debug + PartialEq + Eq + Default + Send,
{
    pub fn new(event_type: QueueType) -> Self {
        Self {
            event_type,
            data: D::default(),
        }
    }

    pub fn with_data(event_type: QueueType, data: D) -> Self {
        Self { event_type, data }
    }
}
