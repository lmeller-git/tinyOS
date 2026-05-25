use crate::kernel::threading::{
    self,
    schedule::{Scheduler, get_scheduler},
    tls,
};

pub fn start_resource_manager() {
    threading::spawn(|| {
        loop {
            // TODO overhaul this entire system to remove busy sping loops.
            // We currently busy spin in
            // a) wait manager
            // b) this thread here
            // c) idle thread
            //
            // b) and c) could be solved with a better scheduler and better wait integration
            // this does not currently work, as someone needs to call reschedule in order for a woken thread to continue.
            // let conditons = &[QueuTypeCondition::with_cond(
            //     QueueType::Timer,
            //     WaitCondition::Time(Duration::from_millis(50) + current_time()),
            // )];

            // wait_manager::add_wait(&tls::task_data().current_pid(), conditons);
            for _ in 0..50 {
                threading::yield_now();
            }
            let scheduler = get_scheduler();
            tls::task_data().cleanup();
            scheduler.reschedule();
            threading::yield_now();
        }
    });
}
