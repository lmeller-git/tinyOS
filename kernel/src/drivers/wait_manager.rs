use conquer_once::spin::OnceCell;

use crate::{
    kernel::threading::{
        self,
        task::TaskID,
        tls,
        wait::{
            MESSAGE_QUEUE,
            QueuTypeCondition,
            QueueType,
            WaitObserver,
            queues::{KEYBOARDQUEUE, KeyBoardQueue, TIMERQUEUE, TimeWaitQueue},
        },
    },
    sync::locks::RwLock,
};

static WAIT_MANAGER: OnceCell<RwLock<WaitObserver>> = OnceCell::uninit();

pub fn add_wait(id: &TaskID, queue_data: &[QueuTypeCondition]) -> Option<()> {
    Some(WAIT_MANAGER.get()?.read().enqueue(id, queue_data))
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
        .add_queue(TIMERQUEUE.get_or_init(TimeWaitQueue::new), QueueType::Timer)
        .unwrap();
    manager
        .add_queue(
            KEYBOARDQUEUE.get_or_init(KeyBoardQueue::new),
            QueueType::KeyBoard,
        )
        .unwrap();
    WAIT_MANAGER.init_once(move || RwLock::new(manager));

    threading::spawn(move || {
        loop {
            WAIT_MANAGER
                .get()
                .unwrap()
                .read()
                .process_signals(MESSAGE_QUEUE.get().unwrap());
        }
    })
    .unwrap();
}
