use crate::arch::x86::mem::VirtAddr;
use lazy_static::lazy_static;
use x86_64::structures::{
    gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
    tss::TaskStateSegment,
};

pub(super) const DOUBLE_FAULT_IST_INDEX: u16 = 0;

struct Selectors {
    code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
    data_selector: SegmentSelector,
}

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            // TODO this should use STACK_SIZE_REQUEST (is currently eq)
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            stack_start + STACK_SIZE as u64
        };
        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));
        // let user_data_selector = gdt.append(Descriptor::user_data_segment());
        let kernel_data_selector = gdt.append(Descriptor::kernel_data_segment());
        // let user_code_selector = gdt.append(Descriptor::user_code_segment());
        (
            gdt,
            Selectors {
                code_selector,
                tss_selector,
                data_selector: kernel_data_selector,
            },
        )
    };
}

pub(super) fn init() {
    use x86_64::instructions::segmentation::{CS, Segment};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        x86_64::instructions::segmentation::SS::set_reg(GDT.1.data_selector);
        load_tss(GDT.1.tss_selector);
    }
}
