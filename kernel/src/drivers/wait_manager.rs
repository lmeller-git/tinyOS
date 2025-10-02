use conquer_once::spin::OnceCell;

use crate::{
    kernel::threading::{
        self,
        task::TaskID,
        tls,
        wait::{
            QueuTypeCondition,
            QueueHandle,
            QueueType,
            WaitObserver,
            queues::{KEYBOARDQUEUE, KeyBoardQueue, TIMERQUEUE, TimeWaitQueue, WaitQueue},
        },
    },
    sync::locks::RwLock,
};

static WAIT_MANAGER: OnceCell<RwLock<WaitObserver>> = OnceCell::uninit();

pub fn add_wait(id: &TaskID, queue_data: &[QueuTypeCondition]) -> Option<()> {
    Some(WAIT_MANAGER.get()?.read().enqueue(id, queue_data))
}

pub fn add_queue(queue: QueueHandle<'static>, queue_type: QueueType) -> Option<()> {
    WAIT_MANAGER.get()?.write().add_queue(queue, queue_type)
}

pub fn remove_queue(queue_type: &QueueType) {
    WAIT_MANAGER.get().unwrap().write().remove_queue(queue_type);
}

pub fn wait_self(queue_data: &[QueuTypeCondition]) -> Option<()> {
    let r = WAIT_MANAGER
        .get()?
        .read()
        .enqueue(&tls::task_data().current_pid(), queue_data);
    threading::yield_now();
    Some(r)
}

pub fn start_wait_managment() {
    threading::wait::init();
    let mut manager = WaitObserver::new();
    manager
        .add_queue(
            QueueHandle::from_borrowed(TIMERQUEUE.get_or_init(TimeWaitQueue::new)),
            QueueType::Timer,
        )
        .unwrap();
    manager
        .add_queue(
            QueueHandle::from_borrowed(KEYBOARDQUEUE.get_or_init(KeyBoardQueue::new)),
            QueueType::KeyBoard,
        )
        .unwrap();
    WAIT_MANAGER.init_once(move || RwLock::new(manager));

    threading::spawn(move || {
        loop {
            WAIT_MANAGER.get().unwrap().read().process_signals();
        }
    })
    .unwrap();
}
