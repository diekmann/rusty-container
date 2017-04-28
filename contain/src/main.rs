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


// a lot copied from man USER_NAMESPACES(7)


use std::thread;

extern "C" fn child_func(args: *mut c_void) -> c_int {
    println!("I'm called from child_func");

    let args = args as *mut ChildArgs;
    let r_pipe_fd = unsafe { (*args).r_pipe_fd };
    let w_pipe_fd = unsafe { (*args).w_pipe_fd };
    println!("r_pipe_fd: {}", r_pipe_fd);

    // run everything in a new thread so exceptions bubble up to rust
    let h = thread::spawn(move || {
        // wait for parent
        println!("waiting for parent to set up mapping, ...");
        assert_eq!(unsafe { libc::close(w_pipe_fd) } , 0);
        unsafe {
            let mut buf = 0u8;
            libc::read(r_pipe_fd, (&mut buf) as *mut u8 as *mut c_void, 1);
        }

        panic!("Oops!");
    });

    match h.join() {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

struct ChildArgs {
    r_pipe_fd: c_int,
    w_pipe_fd: c_int,
}

fn main() {
    println!("Hello, world!");

    let mut pipe_fd = vec![0;2];  /* Pipe used to synchronize parent and child */
    assert_eq!(unsafe { libc::pipe(pipe_fd.as_mut_ptr()) }, 0);
    println!("r pipe: {} w pipe: {}", pipe_fd[0], pipe_fd[1]);
    let ptr_child_args = {
        let child_args = Box::new(ChildArgs { r_pipe_fd: pipe_fd[0], w_pipe_fd: pipe_fd[1]});
        Box::into_raw(child_args) as *mut c_void
    };

    let stack_size = 4 * 1024 * 1024;
    let child_stack = mmap_stack(stack_size);

    // stack grows down
    let child_stack_top = unsafe { child_stack.offset(stack_size as isize) };

    let child_pid = unsafe {
        libc::clone(child_func, child_stack_top, CLONE_NEWUSER|CLONE_NEWNS|CLONE_NEWPID|SIGCHLD, ptr_child_args)
    };
    assert!(child_pid != -1);

    let mut child_status: c_int = 0;
    assert!(unsafe { libc::waitpid(child_pid, &mut child_status, 0) } != -1);
    println!("Child terminated: {}", child_status);

    unsafe {
        drop(Box::from_raw(ptr_child_args));
    }

    //TODO munmap child_stack
}
