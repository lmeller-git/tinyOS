use core::arch::asm;

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe extern "C" fn kernel_return_trampoline() {
    // addr of this is set as the return address for tasks
    // rsp is currently at the topmost addr of tasks stack
    // should:
    // restore cpu context
    // call correct next func
    //
    todo!()
}
