use alloc::boxed::Box;
use core::{fmt::Debug, ops::Deref, ptr::NonNull};

use atomic_pool::pool;
use conquer_once::spin::OnceCell;
use crossbeam::queue::ArrayQueue;
use hashbrown::HashMap;
use nblfq::{HeapBackedQueue, HeaplessQueue};

use crate::{
    arch::interrupt,
    kernel::threading::{
        self,
        task::TaskID,
        tls,
        wait::{condition::WaitCondition, queues::WaitQueue},
    },
    serial_println,
    sync::locks::RwLock,
};

pub mod condition;
pub mod queues;

pub const MAX_WAIT_EVENTS: usize = 20;

pool!(MessagePool: [WaitEvent<u64>; MAX_WAIT_EVENTS]);

// pub static MESSAGE_QUEUE: OnceCell<ArrayQueue<WaitEvent<u64>>> = OnceCell::uninit();
pub static MESSAGE_QUEUE: OnceCell<HeaplessQueue<MAX_WAIT_EVENTS, WaitEvent<u64>>> =
    OnceCell::uninit();

pub fn init() {
    MESSAGE_QUEUE.init_once(|| HeaplessQueue::new());
}

pub fn post_event(event: WaitEvent<u64>) -> Option<()> {
    let event: NonNull<WaitEvent<u64>> =
        atomic_pool::Box::into_raw(atomic_pool::Box::<MessagePool>::new(event)?);
    let event: &'static WaitEvent<u64> = unsafe { event.as_ref() };
    MESSAGE_QUEUE.get().and_then(|queue| queue.push(event).ok())
}

pub fn get_event() -> Option<atomic_pool::Box<MessagePool>> {
    let event = MESSAGE_QUEUE.get()?.pop()?;
    let event = unsafe { atomic_pool::Box::<MessagePool>::from_raw(NonNull::from_ref(event)) };
    Some(event)
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

    pub fn enqueue(&self, id: &TaskID, queue_data: &[QueuTypeCondition]) {
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
        loop {
            if interrupt::without_interrupts(|| {
                queue_data.iter().any(|c| c.cond.is_given())
                    || tls::task_data().try_block(id).is_none()
            }) {
                threading::yield_now();
            } else {
                return;
            }
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

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum QueueType {
    Timer,
    KeyBoard,
    Thread(TaskID),
}

#[derive(Debug, PartialEq, Eq)]
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
