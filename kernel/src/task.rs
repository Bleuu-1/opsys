use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};

const TASK_COUNT: usize = 4;
const STACK_SIZE: usize = 16 * 1024;
pub const EMPTY: u8 = 0;
pub const READY: u8 = 1;
pub const RUNNING: u8 = 2;
pub const EXITED: u8 = 3;

#[repr(C)]
#[derive(Copy, Clone)]
struct TaskControlBlock {
    saved_stack: *mut u64,
    state: u8,
    id: u8,
    _reserved: u16,
    switches: u64,
    cr3: u64,
    kernel_stack_top: u64,
}

const EMPTY_TASK: TaskControlBlock = TaskControlBlock {
    saved_stack: ptr::null_mut(),
    state: EMPTY,
    id: 0,
    _reserved: 0,
    switches: 0,
    cr3: 0,
    kernel_stack_top: 0,
};

#[repr(align(16))]
struct TaskStack {
    _bytes: [u8; STACK_SIZE],
}

static mut TASKS: [TaskControlBlock; TASK_COUNT] = [EMPTY_TASK; TASK_COUNT];
static mut STACKS: [TaskStack; TASK_COUNT - 1] = [
    TaskStack {
        _bytes: [0; STACK_SIZE],
    },
    TaskStack {
        _bytes: [0; STACK_SIZE],
    },
    TaskStack {
        _bytes: [0; STACK_SIZE],
    },
];
static mut CURRENT_TASK: usize = 0;
static mut KERNEL_CR3: u64 = 0;
static mut SCHEDULER_READY: bool = false;
static CONTEXT_SWITCHES: AtomicU64 = AtomicU64::new(0);
static WORKER_COUNTS: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];

pub unsafe fn initialize() {
    KERNEL_CR3 = crate::memory::kernel_address_space();
    let tasks = ptr::addr_of_mut!(TASKS);
    (*tasks)[0] = TaskControlBlock {
        state: RUNNING,
        id: 0,
        cr3: KERNEL_CR3,
        kernel_stack_top: 0xffff_ffff_8009_0000,
        ..EMPTY_TASK
    };

    let stacks = ptr::addr_of_mut!(STACKS).cast::<TaskStack>();
    let entries: [extern "C" fn() -> !; 2] = [worker_a, worker_b];
    for index in 0..2 {
        let stack = stacks.add(index) as usize;
        let stack_top = stack + STACK_SIZE;
        let saved_stack =
            make_initial_context(stack_top - 8, entries[index] as usize, 0x18, 0x10, 32);
        (*tasks)[index + 1] = TaskControlBlock {
            saved_stack,
            state: READY,
            id: (index + 1) as u8,
            _reserved: 0,
            switches: 0,
            cr3: KERNEL_CR3,
            kernel_stack_top: stack_top as u64,
        };
    }
    CURRENT_TASK = 0;
    SCHEDULER_READY = true;
}

/// Add the single demo user process. Its ELF image and user stack already have mappings
/// in a private address space; the kernel stack is reserved in slot 3.
pub unsafe fn spawn_user(entry: u64, user_stack: u64, cr3: u64) -> bool {
    let tasks = ptr::addr_of_mut!(TASKS);
    if (*tasks)[3].state != EMPTY && (*tasks)[3].state != EXITED {
        return false;
    }
    let stacks = ptr::addr_of_mut!(STACKS).cast::<TaskStack>();
    let stack = stacks.add(2) as usize;
    let stack_top = stack + STACK_SIZE;
    let saved_stack = make_user_context(stack_top - 8, entry as usize, user_stack as usize);
    (*tasks)[3] = TaskControlBlock {
        saved_stack,
        state: READY,
        id: 3,
        _reserved: 0,
        switches: 0,
        cr3,
        kernel_stack_top: stack_top as u64,
    };
    true
}

unsafe fn make_initial_context(
    stack_top: usize,
    entry: usize,
    code_selector: u64,
    stack_selector: u64,
    vector: u64,
) -> *mut u64 {
    make_context(
        stack_top,
        entry,
        code_selector,
        stack_selector,
        vector,
        stack_top as u64,
    )
}

unsafe fn make_user_context(stack_top: usize, entry: usize, user_stack: usize) -> *mut u64 {
    make_context(stack_top, entry, 0x2b, 0x23, 49, user_stack as u64)
}

unsafe fn make_context(
    stack_top: usize,
    entry: usize,
    code_selector: u64,
    stack_selector: u64,
    vector: u64,
    stack_pointer: u64,
) -> *mut u64 {
    // irq_common restores these registers and executes iretq. The final SS:RSP pair
    // is required both for first entry into ring 3 and for the kernel worker frames.
    let frame = (stack_top - 21 * core::mem::size_of::<u64>()) as *mut u64;
    for register in 0..15 {
        frame.add(register).write(0);
    }
    frame.add(15).write(vector);
    frame.add(16).write(entry as u64);
    frame.add(17).write(code_selector);
    frame.add(18).write(0x202);
    frame.add(19).write(stack_pointer);
    frame.add(20).write(stack_selector);
    frame
}

pub unsafe fn on_timer(interrupted_stack: *mut u64) -> *mut u64 {
    if !SCHEDULER_READY || interrupted_stack.is_null() {
        return interrupted_stack;
    }
    schedule(interrupted_stack, true)
}

unsafe fn schedule(interrupted_stack: *mut u64, save_current: bool) -> *mut u64 {
    let tasks = ptr::addr_of_mut!(TASKS);
    let current = CURRENT_TASK;
    if save_current && (*tasks)[current].state != EXITED {
        (*tasks)[current].saved_stack = interrupted_stack;
        (*tasks)[current].state = READY;
        (*tasks)[current].switches = (*tasks)[current].switches.wrapping_add(1);
    }

    for offset in 1..=TASK_COUNT {
        let candidate = (current + offset) % TASK_COUNT;
        if (*tasks)[candidate].state == READY {
            (*tasks)[candidate].state = RUNNING;
            CURRENT_TASK = candidate;
            CONTEXT_SWITCHES.fetch_add(1, Ordering::Relaxed);
            return (*tasks)[candidate].saved_stack;
        }
    }
    (*tasks)[current].state = RUNNING;
    (*tasks)[current].saved_stack = interrupted_stack;
    interrupted_stack
}

pub unsafe fn exit_current(interrupted_stack: *mut u64) -> *mut u64 {
    let tasks = ptr::addr_of_mut!(TASKS);
    let current = CURRENT_TASK;
    if current == 3 && (*tasks)[current].state == RUNNING {
        let old_cr3 = (*tasks)[current].cr3;
        (*tasks)[current].state = EXITED;
        (*tasks)[current].cr3 = KERNEL_CR3;
        crate::memory::switch_address_space(KERNEL_CR3);
        crate::memory::destroy_address_space(old_cr3);
    }
    schedule(interrupted_stack, false)
}

pub unsafe fn kill_user() -> bool {
    let task = &mut *ptr::addr_of_mut!(TASKS).cast::<TaskControlBlock>().add(3);
    if task.state == READY {
        let root = task.cr3;
        task.state = EXITED;
        task.cr3 = KERNEL_CR3;
        crate::memory::destroy_address_space(root);
        return true;
    }
    false
}

extern "C" fn worker_a() -> ! {
    loop {
        let count = WORKER_COUNTS[0].fetch_add(1, Ordering::Relaxed) + 1;
        if count & 0xffff == 0 {
            yield_now();
        }
        core::hint::spin_loop();
    }
}

extern "C" fn worker_b() -> ! {
    loop {
        let count = WORKER_COUNTS[1].fetch_add(1, Ordering::Relaxed) + 1;
        if count & 0xffff == 0 {
            yield_now();
        }
        core::hint::spin_loop();
    }
}

pub fn yield_now() {
    unsafe { core::arch::asm!("int 0x30") };
}

pub fn current_id() -> usize {
    unsafe { CURRENT_TASK }
}

pub unsafe fn current_address_space() -> u64 {
    let task = &*ptr::addr_of!(TASKS)
        .cast::<TaskControlBlock>()
        .add(CURRENT_TASK);
    task.cr3
}

pub unsafe fn current_kernel_stack_top() -> u64 {
    let task = &*ptr::addr_of!(TASKS)
        .cast::<TaskControlBlock>()
        .add(CURRENT_TASK);
    task.kernel_stack_top
}

pub fn context_switches() -> u64 {
    CONTEXT_SWITCHES.load(Ordering::Relaxed)
}

pub fn worker_count(index: usize) -> u64 {
    WORKER_COUNTS
        .get(index)
        .map_or(0, |count| count.load(Ordering::Relaxed))
}

pub fn snapshot(id: usize) -> Option<(u8, u8, u64)> {
    if id >= TASK_COUNT {
        return None;
    }
    unsafe {
        let task = &*ptr::addr_of!(TASKS).cast::<TaskControlBlock>().add(id);
        Some((task.id, task.state, task.switches))
    }
}

pub fn count() -> usize {
    TASK_COUNT
}
