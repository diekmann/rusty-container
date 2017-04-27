extern crate libc;

use std::ptr;
use libc::{c_void, size_t};
use libc::{PROT_READ, PROT_WRITE};
use libc::{MAP_PRIVATE, MAP_ANONYMOUS, MAP_GROWSDOWN, MAP_STACK};

fn mmap_stack(stack_size: size_t) -> *mut c_void {
    unsafe {
        let p = libc::mmap(ptr::null_mut(), stack_size, PROT_READ | PROT_WRITE,
                           MAP_PRIVATE | MAP_ANONYMOUS | MAP_GROWSDOWN | MAP_STACK, -1, 0);
        assert!(p != libc::MAP_FAILED);
        p
    }

}

fn main() {
    println!("Hello, world!");

    let stack_size = 4 * 1024 * 1024;
    let child_stack = mmap_stack(stack_size);

    //TODO munmap child_stack
}
