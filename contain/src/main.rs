extern crate libc;
extern crate contain;

use libc::{c_void, c_int};
use libc::{CLONE_NEWUSER, CLONE_NEWNS, CLONE_NEWPID, SIGCHLD};
use std::fs;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::io::{Read, Write};
use std::thread;
use contain::Stack;


// a lot copied from man USER_NAMESPACES(7)


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
            assert_eq!(libc::read(r_pipe_fd, (&mut buf) as *mut u8 as *mut c_void, 1), 0);
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

        let container_root = Path::new("./mntcont");
        if container_root.exists() {
            assert!(container_root.is_dir());
        } else {
            println!("creating {:?} at {}", container_root.file_name().expect("root name"), container_root.to_string_lossy());
            fs::create_dir(container_root).expect("mkdir mntcont");
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
