use crate::kernel::threading::{
    self,
    schedule::{Scheduler, get_scheduler},
    tls,
};

pub fn start_resource_manager() {
    threading::spawn(|| {
        loop {
            // this does not currently work, as someone needs to call reschedule in order for a woken thread to continue.
            // let conditons = &[QueuTypeCondition::with_cond(
            //     QueueType::Timer,
            //     WaitCondition::Time(Duration::from_millis(50) + current_time()),
            // )];

            // wait_manager::add_wait(&tls::task_data().current_pid(), conditons);
            for _ in 0..5 {
                threading::yield_now();
            }
            let scheduler = get_scheduler();
            tls::task_data().cleanup();
            scheduler.reschedule();
            threading::yield_now();
        }
    });
}
