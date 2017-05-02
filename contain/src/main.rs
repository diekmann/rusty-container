extern crate libc;

use std::ptr;
use libc::{c_void, size_t, c_int};
use libc::{PROT_READ, PROT_WRITE, PROT_NONE};
use libc::{MAP_PRIVATE, MAP_ANONYMOUS, MAP_GROWSDOWN, MAP_STACK};
use libc::{CLONE_NEWUSER, CLONE_NEWNS, CLONE_NEWPID, SIGCHLD};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};


struct Stack {
    p: *mut c_void,
    len: usize,
    pub top: *mut c_void,
}

impl Stack {
    // stack size in pages
    fn new(stack_size: usize) -> Self {
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

        {
            println!("Child status");
            let mut fstatus = File::open("/proc/self/status").expect("open status");
            let mut contents = String::new();
            fstatus.read_to_string(&mut contents).expect("read status");
            for l in contents.lines(){
                if l.starts_with("Seccomp") || l.starts_with("Cap"){
                    println!("{}", l);
                }
            }
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

fn write(path: &String, content: &[u8]) {
    println!("Writing to {}", path);
    let mut file = OpenOptions::new().read(false).write(true).create(false).open(path).expect("open");
    // needs to happen in one write call
    assert_eq!(file.write(content).expect("write"), content.len());
    //file gets dropped
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


    let child_stack = Stack::new(512);

    println!("cloning");
    let child_pid = unsafe {
        libc::clone(child_func, child_stack.top, CLONE_NEWUSER|CLONE_NEWNS|CLONE_NEWPID|SIGCHLD, ptr_child_args)
    };
    assert!(child_pid != -1);

    let uid = unsafe { libc::getuid() };
    write(&format!("/proc/{}/uid_map", child_pid), &format!("0 {} 1", uid).into_bytes());

    //proc_setgroups_write man user_namespaces
    write(&format!("/proc/{}/setgroups", child_pid), &format!("deny").into_bytes());

    let gid = unsafe { libc::getgid() };
    write(&format!("/proc/{}/gid_map", child_pid), &format!("0 {} 1", gid).into_bytes());

    //unleash child
    assert_eq!(unsafe { libc::close(pipe_fd[1]) }, 0);


    let mut child_status: c_int = 0;
    assert!(unsafe { libc::waitpid(child_pid, &mut child_status, 0) } != -1);
    println!("Child terminated: {}", child_status);

    unsafe {
        drop(Box::from_raw(ptr_child_args));
    }

    //child_stack gets unmapped
}
