extern crate libc;

use std::ptr;
use libc::{c_void, size_t, c_int};
use libc::{PROT_READ, PROT_WRITE};
use libc::{MAP_PRIVATE, MAP_ANONYMOUS, MAP_GROWSDOWN, MAP_STACK};
use libc::{CLONE_NEWUSER, CLONE_NEWNS, CLONE_NEWPID, SIGCHLD};

fn mmap_stack(stack_size: size_t) -> *mut c_void {
    unsafe {
        let p = libc::mmap(ptr::null_mut(), stack_size, PROT_READ | PROT_WRITE,
                           MAP_PRIVATE | MAP_ANONYMOUS | MAP_GROWSDOWN | MAP_STACK, -1, 0);
        assert!(p != libc::MAP_FAILED);
        //TODO add guard pages
        p
    }

}


use std::thread;

extern "C" fn child_func(args: *mut c_void) -> c_int {
    println!("I'm called from child_func");
    // run everything in a new thread so exceptions bubble up to rust
    let h = thread::spawn(|| {
        panic!("Oops!");
    });

    match h.join() {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

fn main() {
    println!("Hello, world!");

    let stack_size = 4 * 1024 * 1024;
    let child_stack = mmap_stack(stack_size);

    // stack grows down
    let child_stack_top = unsafe { child_stack.offset(stack_size as isize) };

    let child_pid = unsafe { 
        libc::clone(child_func, child_stack_top, CLONE_NEWUSER|CLONE_NEWNS|CLONE_NEWPID|SIGCHLD, ptr::null_mut())
    };
    assert!(child_pid != -1);

    let mut child_status: c_int = 0;
    assert!(unsafe { libc::waitpid(child_pid, &mut child_status, 0) } != -1);
    println!("Child terminated: {}", child_status);

    //TODO munmap child_stack
}
