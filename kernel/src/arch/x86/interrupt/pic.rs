use alloc::{format, sync::Arc, vec::Vec};
use core::{
    ptr::{NonNull, null_mut},
    sync::atomic::{AtomicU64, Ordering},
};

use acpi::{AcpiTables, PciConfigRegions};
use conquer_once::spin::OnceCell;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::port::Port;

use super::idt::InterruptIndex;
use crate::{
    arch::x86::mem::*,
    bootinfo,
    create_device_file,
    kernel::{
        fd::{FileRepr, IOCapable},
        fs::{Path, PathBuf},
        io::{IOError, Read, Write},
        mem::paging::{PAGETABLE, map_region_generic},
        threading::wait::QueuTypeCondition,
    },
    println,
    register_device_file,
    serial_println,
};

lazy_static! {
    pub static ref LAPIC_ADDR: Mutex<LAPICAddress> = Mutex::new(LAPICAddress::new()); // Needs to be initialized
}

pub static CYCLES_PER_SECOND: AtomicU64 = AtomicU64::new(0);
pub const CYCLES_PER_TICK: u32 = 1000000;
pub struct LAPICAddress {
    address: *mut u32,
}

unsafe impl Send for LAPICAddress {}
unsafe impl Sync for LAPICAddress {}

impl LAPICAddress {
    pub fn new() -> Self {
        Self {
            address: core::ptr::null_mut(),
        }
    }
}

// https://wiki.osdev.org/APIC
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
#[repr(isize)]
#[allow(dead_code)]
pub enum APICOffset {
    R0x00 = 0x0,      // RESERVED = 0x00
    R0x10 = 0x10,     // RESERVED = 0x10
    Ir = 0x20,        // ID Register
    Vr = 0x30,        // Version Register
    R0x40 = 0x40,     // RESERVED = 0x40
    R0x50 = 0x50,     // RESERVED = 0x50
    R0x60 = 0x60,     // RESERVED = 0x60
    R0x70 = 0x70,     // RESERVED = 0x70
    Tpr = 0x80,       // Text Priority Register
    Apr = 0x90,       // Arbitration Priority Register
    Ppr = 0xA0,       // Processor Priority Register
    Eoi = 0xB0,       // End of Interrupt
    Rrd = 0xC0,       // Remote Read Register
    Ldr = 0xD0,       // Logical Destination Register
    Dfr = 0xE0,       // DFR
    Svr = 0xF0,       // Spurious (Interrupt) Vector Register
    Isr1 = 0x100,     // In-Service Register 1
    Isr2 = 0x110,     // In-Service Register 2
    Isr3 = 0x120,     // In-Service Register 3
    Isr4 = 0x130,     // In-Service Register 4
    Isr5 = 0x140,     // In-Service Register 5
    Isr6 = 0x150,     // In-Service Register 6
    Isr7 = 0x160,     // In-Service Register 7
    Isr8 = 0x170,     // In-Service Register 8
    Tmr1 = 0x180,     // Trigger Mode Register 1
    Tmr2 = 0x190,     // Trigger Mode Register 2
    Tmr3 = 0x1A0,     // Trigger Mode Register 3
    Tmr4 = 0x1B0,     // Trigger Mode Register 4
    Tmr5 = 0x1C0,     // Trigger Mode Register 5
    Tmr6 = 0x1D0,     // Trigger Mode Register 6
    Tmr7 = 0x1E0,     // Trigger Mode Register 7
    Tmr8 = 0x1F0,     // Trigger Mode Register 8
    Irr1 = 0x200,     // Interrupt Request Register 1
    Irr2 = 0x210,     // Interrupt Request Register 2
    Irr3 = 0x220,     // Interrupt Request Register 3
    Irr4 = 0x230,     // Interrupt Request Register 4
    Irr5 = 0x240,     // Interrupt Request Register 5
    Irr6 = 0x250,     // Interrupt Request Register 6
    Irr7 = 0x260,     // Interrupt Request Register 7
    Irr8 = 0x270,     // Interrupt Request Register 8
    Esr = 0x280,      // Error Status Register
    R0x290 = 0x290,   // RESERVED = 0x290
    R0x2A0 = 0x2A0,   // RESERVED = 0x2A0
    R0x2B0 = 0x2B0,   // RESERVED = 0x2B0
    R0x2C0 = 0x2C0,   // RESERVED = 0x2C0
    R0x2D0 = 0x2D0,   // RESERVED = 0x2D0
    R0x2E0 = 0x2E0,   // RESERVED = 0x2E0
    LvtCmci = 0x2F0,  // LVT Corrected Machine Check Interrupt (CMCI) Register
    Icr1 = 0x300,     // Interrupt Command Register 1
    Icr2 = 0x310,     // Interrupt Command Register 2
    LvtT = 0x320,     // LVT Timer Register
    LvtTsr = 0x330,   // LVT Thermal Sensor Register
    LvtPmcr = 0x340,  // LVT Performance Monitoring Counters Register
    LvtLint0 = 0x350, // LVT LINT0 Register
    LvtLint1 = 0x360, // LVT LINT1 Register
    LvtE = 0x370,     // LVT Error Register
    Ticr = 0x380,     // Initial Count Register (for Timer)
    Tccr = 0x390,     // Current Count Register (for Timer)
    R0x3A0 = 0x3A0,   // RESERVED = 0x3A0
    R0x3B0 = 0x3B0,   // RESERVED = 0x3B0
    R0x3C0 = 0x3C0,   // RESERVED = 0x3C0
    R0x3D0 = 0x3D0,   // RESERVED = 0x3D0
    Tdcr = 0x3E0,     // Divide Configuration Register (for Timer)
    R0x3F0 = 0x3F0,   // RESERVED = 0x3F0
}

#[derive(Clone)]
struct Foo;

impl acpi::AcpiHandler for Foo {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> acpi::PhysicalMapping<Self, T> {
        let phys_start = PhysAddr::new(physical_address as u64);
        let virt_start =
            VirtAddr::new(physical_address as u64 + crate::bootinfo::get_phys_offset());

        let start_page: Page<Size4KiB> = Page::containing_address(virt_start);
        let end_page: Page<Size4KiB> = Page::containing_address(virt_start + size as u64 - 1);
        {
            let mut mapper = crate::kernel::mem::paging::PAGETABLE.lock();
            let mut frame_allocator = crate::kernel::mem::paging::get_frame_alloc().lock();

            for page in Page::range_inclusive(start_page, end_page) {
                let frame = PhysFrame::containing_address(PhysAddr::new(
                    page.start_address().as_u64() - crate::bootinfo::get_phys_offset(),
                ));
                let flags =
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;

                // if mapper.translate_page(page).is_ok() {
                //     continue;
                // }

                unsafe {
                    match mapper.map_to(page, frame, flags, &mut *frame_allocator) {
                        Ok(f) => f.flush(),
                        Err(mapper::MapToError::PageAlreadyMapped(_)) => {}
                        Err(e) => panic!("{:#?}", e),
                    }
                }
            }
        }
        unsafe {
            acpi::PhysicalMapping::new(
                physical_address,
                NonNull::new(virt_start.as_mut_ptr()).unwrap(),
                size,
                size,
                self.clone(),
            )
        }
    }

    fn unmap_physical_region<T>(region: &acpi::PhysicalMapping<Self, T>) {
        // TODO
        return;
        let start = region.physical_start();
        let size = region.mapped_length();
        for phys in (start..start + size).step_by(Size4KiB::SIZE as usize) {
            let vaddr = phys as u64 + bootinfo::get_phys_offset();
            let page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(vaddr));
            let (_frame, flush) = crate::kernel::mem::paging::PAGETABLE
                .lock()
                .unmap(page)
                .unwrap();
            flush.flush();
        }
    }
}

fn map_no_cache(
    physical_address: u64,
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> VirtAddr {
    let physical_address = PhysAddr::new(physical_address);
    let page = Page::containing_address(VirtAddr::new(
        physical_address.as_u64() + bootinfo::get_phys_offset(),
    ));
    let frame = PhysFrame::containing_address(physical_address);

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;

    unsafe {
        mapper
            .map_to(page, frame, flags, frame_allocator)
            .expect("APIC mapping failed")
            .flush();
    }

    page.start_address()
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn init_io_apic(
    io_apic_addr: usize,
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    let virt_addr = map_no_cache(io_apic_addr as u64, mapper, frame_allocator);

    let ioapic_pointer = virt_addr.as_mut_ptr::<u32>();

    ioapic_pointer.offset(0).write_volatile(0x12);
    ioapic_pointer
        .offset(4)
        .write_volatile(InterruptIndex::Keyboard as u32);

    for dev in PCI_DEVICES.get().unwrap() {
        if let Some(gsi) = dev.gsi {
            let vec = 0x40 + gsi;
            serial_println!("Routing PCI GSI {} to IDT Vector {:#X}", gsi, vec);

            unsafe {
                io_apic_set_routing(virt_addr, gsi, vec);
            }
        }
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn io_apic_set_routing(io_apic_addr: VirtAddr, gsi: u8, vector: u8) {
    let select_ptr = io_apic_addr.as_mut_ptr::<u32>();
    let window_ptr = (io_apic_addr.as_u64() + 0x10) as *mut u32;

    let low_index = 0x10 + (gsi as u32 * 2);
    let high_index = low_index + 1;

    let low_bits = (vector as u32) | (1 << 13) | (1 << 15);
    let high_bits = (0 << 24) as u32;

    serial_println!(
        "routing gsi {} and vec {:#x} with low idx {}, bits {:#x} high idx {}, bits {:#x} to select {:#x}, window {:#x}",
        gsi,
        vector,
        low_index,
        low_bits,
        high_index,
        high_bits,
        select_ptr as usize,
        window_ptr as usize
    );

    core::ptr::write_volatile(select_ptr, low_index);
    core::ptr::write_volatile(window_ptr, low_bits);

    core::ptr::write_volatile(select_ptr, high_index);
    core::ptr::write_volatile(window_ptr, high_bits);
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn init_timer(lapic_pointer: *mut u32) {
    let svr = lapic_pointer.offset(APICOffset::Svr as isize / 4);
    svr.write_volatile(svr.read_volatile() | 0x100);

    // Configure timer
    // Vector 0x20, masked (bit 16 = 1)
    let lvt_timer = lapic_pointer.offset(APICOffset::LvtT as isize / 4);
    lvt_timer.write_volatile(0x20 | (1 << 16));

    // Set divider to 16
    let tdcr = lapic_pointer.offset(APICOffset::Tdcr as isize / 4);
    tdcr.write_volatile(0x3);

    // Set initial count - smaller value for more frequent interrupts
}

pub unsafe fn set_timer_count(ptr: *mut u32, count: u32) {
    unsafe {
        let ticr = ptr.offset(APICOffset::Ticr as isize / 4);
        ticr.write_volatile(count);
    }
}

pub unsafe fn enable_periodic_timer(ptr: *mut u32) {
    unsafe {
        let lvt_timer = ptr.offset(APICOffset::LvtT as isize / 4);
        let mut val = lvt_timer.read_volatile();
        val |= 1 << 17;
        lvt_timer.write_volatile(val);
    }
}

pub unsafe fn enable_one_shot_mode(ptr: *mut u32) {
    unsafe {
        let lvt_timer = ptr.offset(APICOffset::LvtT as isize / 4);
        let mut val = lvt_timer.read_volatile();
        val &= !(1 << 17);
        lvt_timer.write_volatile(val);
    }
}

pub fn enable_timer() {
    let lapic_ptr = LAPIC_ADDR.lock().address;
    unsafe {
        let lvt_timer = lapic_ptr.offset(APICOffset::LvtT as isize / 4);
        let mut val = lvt_timer.read_volatile();
        val &= !(1 << 16);
        lvt_timer.write_volatile(val);
    }
}

pub fn disable_timer() {
    let lapic_ptr = LAPIC_ADDR.lock().address;
    unsafe {
        let lvt_timer = lapic_ptr.offset(APICOffset::LvtT as isize / 4);
        let mut val = lvt_timer.read_volatile();
        val |= 1 << 16;
        lvt_timer.write_volatile(val);
    }
}

pub unsafe fn calibrate_apic_timer(ptr: *mut u32) {
    unsafe { enable_one_shot_mode(ptr) };

    let test_count = 10_000_000;
    enable_timer();

    // read tsc
    let tsc_start = rdtsc();
    unsafe { set_timer_count(ptr, test_count) };
    // wait for timer to finish
    unsafe {
        let tccr = ptr.offset(APICOffset::Tccr as isize / 4);
        while tccr.read_volatile() != 0 {}
    };
    // read tsc
    let tsc_end = rdtsc();
    disable_timer();
    let delta_tsc = tsc_end - tsc_start;
    let cpuid = raw_cpuid::CpuId::new();
    let tsz_freq = if let Some(tsc) = cpuid.get_tsc_info()
        && let Some(freq) = tsc.tsc_frequency()
    {
        freq
    } else if let Some(base) = cpuid.get_processor_frequency_info() {
        base.processor_max_frequency() as u64 * 1_000_000
    } else {
        serial_println!("huhu");
        // TODO get actual freq, for noe just some random value (3 GHz)
        3_000_000_000
    };
    let apic_ticks_per_s = (test_count as u64 * tsz_freq) / delta_tsc;
    CYCLES_PER_SECOND.store(apic_ticks_per_s, Ordering::Release);
}

fn rdtsc() -> u64 {
    let hi: u32;
    let lo: u32;
    unsafe {
        core::arch::asm!(
            "cpuid", // serialize
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((hi as u64) << 32) | lo as u64
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn init_keyboard(lapic_pointer: *mut u32) {
    let keyboard_register = lapic_pointer.offset(APICOffset::LvtLint1 as isize / 4);
    keyboard_register.write_volatile(InterruptIndex::Keyboard as u32);
}

fn drain_keyboard() {
    let _: u8 = unsafe { Port::new(0x60).read() };
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn init_local_apic(
    local_apic_addr: usize,
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    let virtual_address = map_no_cache(local_apic_addr as u64, mapper, frame_allocator);
    let lapic_pointer = virtual_address.as_mut_ptr::<u32>();

    LAPIC_ADDR.lock().address = lapic_pointer;
    init_timer(lapic_pointer);
    calibrate_apic_timer(lapic_pointer);
    enable_periodic_timer(lapic_pointer);
    set_timer_count(lapic_pointer, CYCLES_PER_TICK);
    init_keyboard(lapic_pointer);
    drain_keyboard();
}

fn disable_pic() {
    // Disable any unneeded PIC features, such as timer or keyboard to prevent it from firing interrupts
    use x86_64::instructions::port::Port;

    unsafe {
        Port::<u8>::new(0xA1).write(0xFF); // PIC2 (Slave PIC)
    }
}

// TODO
// move the pci device stuff into a separate file

static PCI_DEVICES: OnceCell<Vec<PciDevice>> = OnceCell::uninit();

#[derive(Debug, Clone)]
pub struct PciDevice {
    pub base: VirtAddr,
    pub gsi: Option<u8>,
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub bars: [PciBar; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PciBar {
    Memory { address: PhysAddr, size: usize },
    Io { port: u32, size: usize },
    None,
}

const PCI_DEVICE_MAX_OFFSET: usize = 4096;

impl FileRepr for PciDevice {
    fn node_type(&self) -> crate::kernel::fs::NodeType {
        crate::kernel::fs::NodeType::File
    }
}

impl IOCapable for PciDevice {}

impl Read for PciDevice {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        if offset > PCI_DEVICE_MAX_OFFSET {
            return Err(IOError::simple(crate::kernel::fs::FSErrorKind::EOF));
        }
        let mut written = 0;
        for (i, byte) in buf.iter_mut().enumerate() {
            if offset + written > PCI_DEVICE_MAX_OFFSET {
                break;
            }
            *byte = unsafe { self.base.as_mut_ptr::<u8>().add(offset + i).read_volatile() };
            written += 1;
        }
        Ok(written)
    }
}

impl Write for PciDevice {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        if offset + buf.len() > PCI_DEVICE_MAX_OFFSET {
            return Err(IOError::simple(crate::kernel::fs::FSErrorKind::EOF));
        }
        for (i, byte) in buf.iter().enumerate() {
            unsafe {
                self.base
                    .as_mut_ptr::<u8>()
                    .add(offset + i)
                    .write_volatile(*byte);
            }
        }
        Ok(buf.len())
    }
}

#[derive(Debug)]
pub struct PciDeviceInterruptWaiter {}

impl FileRepr for PciDeviceInterruptWaiter {
    fn node_type(&self) -> crate::kernel::fs::NodeType {
        crate::kernel::fs::NodeType::File
    }

    fn get_waiter(&self) -> Option<crate::kernel::threading::wait::QueuTypeCondition> {
        todo!()
    }
}

impl IOCapable for PciDeviceInterruptWaiter {}

impl Read for PciDeviceInterruptWaiter {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

impl Write for PciDeviceInterruptWaiter {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

#[derive(Debug)]
pub struct PciDeviceBarFile {
    inner: PciBar,
}

impl FileRepr for PciDeviceBarFile {
    fn node_type(&self) -> crate::kernel::fs::NodeType {
        crate::kernel::fs::NodeType::File
    }

    fn as_raw_parts(&self) -> (*mut u8, usize) {
        match self.inner {
            PciBar::Memory { address, size } => (
                (address.as_u64() + bootinfo::get_phys_offset()) as *mut u8,
                size,
            ),
            _ => (null_mut(), 0),
        }
    }

    fn on_open(&self, _meta: crate::kernel::fd::FileMetadata) {
        // map self into mem
        // already identity mapped, can simply call mmap?

        // match self.inner {
        //     PciBar::Memory { address, size } => {
        //         _ = map_region_generic(
        //             VirtAddr::new(address.as_u64() + bootinfo::get_phys_offset()),
        //             size,
        //             PageTableFlags::NO_CACHE | PageTableFlags::WRITABLE | PageTableFlags::PRESENT,
        //             &mut *PAGETABLE.lock(),
        //             |idx| PhysFrame::containing_address(address + Size4KiB::SIZE * idx as u64),
        //         );
        //     }
        //     _ => {}
        // }
    }

    fn on_close(&self, _meta: crate::kernel::fd::FileMetadata) {
        // unmap the mapped pages
        // TODO
    }
}

impl IOCapable for PciDeviceBarFile {}

impl Read for PciDeviceBarFile {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

impl Write for PciDeviceBarFile {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

#[derive(Debug)]
pub struct PciDeviceDMA {}

impl FileRepr for PciDeviceDMA {
    fn node_type(&self) -> crate::kernel::fs::NodeType {
        crate::kernel::fs::NodeType::File
    }
}

impl IOCapable for PciDeviceDMA {}

impl Read for PciDeviceDMA {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

impl Write for PciDeviceDMA {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

// TODO
// for now more or less random
const DMA_MEMORY_START: usize = 0x0200_0000;
const DMA_PAGES_PER_DEVICE: usize = 16;
const DMA_MEMORY_END: usize = 0x0300_0000;

pub fn create_proc_pcie_entry() {
    // for each device we set up a dir in /proc/pci containing
    // config -- file which points to mapped status registers, ...
    //  -> this simply exposes read + write, which reads/writes (validated) at device.base + offset.
    //  This is already mapped from acpi parsing
    // bar0-6 for each found bar pointing to the mapped bar
    //  -> these should be read via mmap (but may expose simple read write for convenience).
    //  These are NOT mapped prior to opening / mmap
    // dma points to dma for the device if it exists
    //  -> should be read via mmap
    //  -> should be mapped into some contiguous region at startup in kernel addr-space
    // irq - allows to wait for an interrupt of the device
    //  -> read will wait the process until the next data arrives
    let mut mapped_devs = 0;
    for device in PCI_DEVICES.get().unwrap() {
        let mut dev_path = Path::new(&format!(
            "/pci/{:02x}:{:02x}.{}",
            device.bus, device.slot, device.func
        ))
        .to_owned();

        dev_path.push("config");
        create_device_file!(device, dev_path.as_ref());
        dev_path.up();

        dev_path.push("irq");
        create_device_file!(Arc::new(PciDeviceInterruptWaiter {}), dev_path.as_ref());
        dev_path.up();

        for (i, bar) in device.bars.iter().enumerate() {
            if *bar == PciBar::None {
                continue;
            }

            dev_path.push(format!("bar{i}").as_str());
            create_device_file!(
                Arc::new(PciDeviceBarFile { inner: bar.clone() }),
                dev_path.as_ref()
            );
            dev_path.up();
        }

        // DMA
        // may not be necessary for all devices?
        if true
            && mapped_devs * DMA_PAGES_PER_DEVICE * Size4KiB::SIZE as usize + DMA_MEMORY_START
                < DMA_MEMORY_END
        {
            let mut page_table = crate::kernel::mem::paging::PAGETABLE.lock();

            map_region_generic(
                VirtAddr::new(
                    (DMA_MEMORY_START
                        + mapped_devs * Size4KiB::SIZE as usize * DMA_PAGES_PER_DEVICE)
                        as u64
                        + bootinfo::get_phys_offset(),
                ),
                DMA_PAGES_PER_DEVICE * Size4KiB::SIZE as usize,
                PageTableFlags::NO_CACHE
                    | PageTableFlags::WRITE_THROUGH
                    | PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE,
                &mut *page_table,
                |idx| {
                    PhysFrame::containing_address(PhysAddr::new(
                        (DMA_MEMORY_START
                            + idx * Size4KiB::SIZE as usize
                            + mapped_devs * Size4KiB::SIZE as usize * DMA_PAGES_PER_DEVICE)
                            as u64,
                    ))
                },
            );

            dev_path.push("dma");
            create_device_file!(Arc::new(PciDeviceDMA {}), dev_path.as_ref());
            mapped_devs += 1;
        }
    }
}

fn parse_pci_device(
    base: VirtAddr,
    bus: u8,
    slot: u8,
    vendor_id: u16,
    device_id: u16,
) -> Option<PciDevice> {
    let ptr = base.as_mut_ptr::<u32>();
    // [Class (8b)][Subclass (8b)][ProgIF (8b)][Revision (8b)]
    let class_reg = unsafe { core::ptr::read_volatile(ptr.add(0x08 / 4)) };
    let class = (class_reg >> 24) as u8;
    let subclass = (class_reg >> 16) as u8;
    let prog_if = (class_reg >> 8) as u8;

    let intr_reg = unsafe { core::ptr::read_volatile(ptr.add(0x3C / 4)) };
    let pin = ((intr_reg >> 8) & 0xFF) as u8;
    let line = (intr_reg & 0xFF) as u8;
    let line = if pin != 0 {
        serial_println!("Device uses Pin {}, GSI Hint: {}", pin, line);
        Some(line)
    } else {
        None
    };

    // header type
    let header_reg = unsafe { core::ptr::read_volatile(ptr.add(0x0C / 4)) };
    let header_type = (header_reg >> 16) as u8 & 0x7F;

    let mut bars = [PciBar::None; 6];

    // read BAR for header 0 type
    if header_type == 0 {
        let mut i = 0;
        while i < 6 {
            let bar_offset = 0x10 + (i * 4);
            let bar_low = unsafe { core::ptr::read_volatile(ptr.add(bar_offset / 4)) };

            if bar_low == 0 {
                i += 1;
                continue;
            }

            // 0 = Memory, 1 = IO
            if (bar_low & 0x1) == 0 {
                // Memory BAR
                let is_64bit = (bar_low & 0b110) == 0b100;
                let mut addr = (bar_low & !0xF) as u64;

                unsafe {
                    core::ptr::write_volatile(ptr.add(bar_offset / 4), 0xFFFFFFFF);
                    let size_mask = core::ptr::read_volatile(ptr.add(bar_offset / 4));
                    core::ptr::write_volatile(ptr.add(bar_offset / 4), bar_low);
                    let size = (!(size_mask & !0xF)).wrapping_add(1) as usize;

                    if is_64bit && i < 5 {
                        let bar_high = core::ptr::read_volatile(ptr.add((bar_offset + 4) / 4));
                        addr |= (bar_high as u64) << 32;
                        bars[i] = PciBar::Memory {
                            address: PhysAddr::new(addr),
                            size,
                        };
                        // Skip next BAR as it's part of this 64-bit addr
                        i += 2;
                    } else {
                        bars[i] = PciBar::Memory {
                            address: PhysAddr::new(addr),
                            size,
                        };
                        i += 1;
                    }
                }
            } else {
                // IO BAR
                let port = bar_low & !0x3;
                // TODO calculate size
                bars[i] = PciBar::Io { port, size: 0 };
                i += 1;
            }
        }
    }

    Some(PciDevice {
        base,
        gsi: line,
        bus,
        slot,
        func: 0, // TODO
        vendor_id,
        device_id,
        class,
        subclass,
        prog_if,
        bars,
    })
}

/// This function will lock mapper and frame alloc
fn scan_pci_regions(table: &AcpiTables<Foo>) {
    let pci_regions = PciConfigRegions::new(table).expect("failed to parse pci config regions");

    for region in pci_regions.iter() {
        serial_println!(
            "PCI Segment {}: Physical Base {:#X} (Buses {:?})",
            region.segment_group,
            region.physical_address,
            region.bus_range,
        );
    }

    let mut devices = Vec::new();

    for bus in 0..=255 {
        for device in 0..32 {
            // for now just check function 0
            // TODO
            if let Some(phys_addr) = pci_regions.physical_address(0, bus, device, 0) {
                let mut page_table = crate::kernel::mem::paging::PAGETABLE.lock();
                let mut frame_allocator = crate::kernel::mem::paging::get_frame_alloc().lock();

                let virt_addr = map_no_cache(phys_addr, &mut *page_table, &mut *frame_allocator);

                drop(page_table);
                drop(frame_allocator);

                let ptr = virt_addr.as_ptr::<u32>();
                let id_reg = unsafe { core::ptr::read_volatile(ptr) };

                let vendor = (id_reg & 0xFFFF) as u16;
                let device_id = (id_reg >> 16) as u16;

                if vendor != 0xFFFF {
                    serial_println!(
                        "Found Device at {}:{}:0 - ID {:04x}:{:04x}",
                        bus,
                        device,
                        vendor,
                        device_id
                    );

                    if let Some(dev) = parse_pci_device(virt_addr, bus, device, vendor, device_id) {
                        serial_println!("parsed device {:?}", dev);
                        devices.push(dev);
                    }
                }
            }
        }
    }
    PCI_DEVICES.init_once(|| devices);
}

pub(super) fn init_apic() {
    //-> acpi::PhysicalMapping<Foo, Madt> {
    println!("initig");
    let handler = Foo;
    let acpi_table = unsafe { AcpiTables::from_rsdp(handler, bootinfo::rdsp_addr()).unwrap() };
    println!("acpi parsed 0");
    let platform_info = acpi_table.platform_info().unwrap();

    println!("acpi parsed");

    // let phys_apic_base: u32 = acpi_table.find_table::<Madt>().unwrap().local_apic_address;

    match platform_info.interrupt_model {
        acpi::InterruptModel::Apic(apic) => {
            scan_pci_regions(&acpi_table);
            let mut page_table = crate::kernel::mem::paging::PAGETABLE.lock();
            let mut frame_allocator = crate::kernel::mem::paging::get_frame_alloc().lock();

            let io_apic_addr = apic.io_apics[0].address;
            unsafe {
                init_io_apic(
                    io_apic_addr as usize,
                    &mut *page_table,
                    &mut *frame_allocator,
                );
            };
            println!("io init");

            let local_apic_addr = apic.local_apic_address;
            // cross_println!("addr loc: {:#?}", local_apic_addr as *const u32);
            // cross_println!("phys: {:#?}", bootinfo::get_phys_offset() as *const u32);
            unsafe {
                init_local_apic(
                    local_apic_addr as usize,
                    &mut *page_table,
                    &mut *frame_allocator,
                )
            };
            println!("local init");
        }
        acpi::InterruptModel::Unknown => {
            todo!()
        }
        _ => {
            todo!()
        }
    }
    disable_pic();
    // let t = acpi_table.find_table::<Madt>().unwrap();
    // t
}

#[unsafe(no_mangle)]
pub fn end_interrupt() {
    unsafe {
        let lapic_ptr = LAPIC_ADDR.lock().address;
        // serial_println!("addr: {:#?}", lapic_ptr);
        lapic_ptr
            .offset(APICOffset::Eoi as isize / 4)
            .write_volatile(0);
    }
}
