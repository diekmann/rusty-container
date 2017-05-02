extern crate libc;

use std::ptr;
use libc::c_void;
use libc::{PROT_READ, PROT_WRITE, PROT_NONE};
use libc::{MAP_PRIVATE, MAP_ANONYMOUS, MAP_GROWSDOWN, MAP_STACK};

pub struct Stack {
    p: *mut c_void,
    len: usize,
    pub top: *mut c_void,
}

impl Stack {
    // stack size in pages
    pub fn new(stack_size: usize) -> Self {
        let pagesize = unsafe { libc::sysconf(libc::_SC_PAGE_SIZE) };
        assert!(pagesize > 1);
        let pagesize = pagesize as usize;

        let stack_size = stack_size * pagesize;
        println!("Pagesize is {}k, allocating stack of size {}k", pagesize/1024, stack_size/1024);
        let len = stack_size + 2*pagesize; // guard pages

        let p = unsafe {
            let p = libc::mmap(ptr::null_mut(),
                               len,
                               PROT_READ | PROT_WRITE,
                               MAP_PRIVATE | MAP_ANONYMOUS | MAP_GROWSDOWN | MAP_STACK, -1, 0);
            assert!(p != libc::MAP_FAILED);
            assert_eq!(p as usize % pagesize, 0); // aligned to page boundary
            p
        };

        // guard pages
        assert_eq!(unsafe { libc::mprotect(p, pagesize, PROT_NONE) }, 0);
        let start_of_end = p as usize + pagesize + (stack_size / pagesize)*pagesize;
        assert_eq!(unsafe { libc::mprotect(start_of_end as *mut c_void, pagesize, PROT_NONE) }, 0);

        // stack grows down
        let stack_top = p as usize + pagesize + stack_size;
        assert_eq!(stack_top % pagesize, 0);

        Stack{ p:p, len:len, top:stack_top as *mut c_void }
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        println!("munmap stack");
        assert_eq!(unsafe { libc::munmap(self.p, self.len) }, 0);
    }
}
