extern crate libc;
extern crate contain;

use libc::{c_void, c_int, c_ulong, c_char};
use std::ptr;
use libc::{MS_REC, MS_PRIVATE, MS_NOSUID, MS_NODEV, MS_NOEXEC};
use libc::{CLONE_NEWUSER, CLONE_NEWNS, CLONE_NEWPID, SIGCHLD};
use std::fs;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::io::{Read, Write};
use std::thread;
use std::ffi::{CString, CStr};
use std::os::unix::ffi::OsStrExt;
use contain::Stack;


//path to cstr
fn p2cstr<P: AsRef<Path>>(po: Option<P>) -> Option<CString> {
    po.map( |p|
        //http://stackoverflow.com/questions/38948669/whats-the-most-direct-way-to-convert-a-path-to-a-c-char
        // TODO possible without copying?
        CString::new(p.as_ref().as_os_str().as_bytes()).expect("string contains NULLs")
    )
}

// CString Option to pointer
fn cstr2p(o: Option<CString>) -> *const c_char {
    o.map_or(ptr::null(), |x| x.as_ptr())
}

fn domount<P: AsRef<Path>>(source: Option<P>, target: Option<P>, filesystemtype: Option<&str>, mountflags: c_ulong) -> () {
    let source = p2cstr(source.as_ref());
    let target = p2cstr(target.as_ref());
    let fstype = match filesystemtype {
            None => ptr::null(),
            Some(fst) => CString::new(fst).expect("fstypes has NULLs").as_ptr(),
        };
    let r = unsafe { libc::mount(cstr2p(source), cstr2p(target), fstype, mountflags, ptr::null()) };
    assert_eq!(r, 0);
}

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
            println!("creating {:?}", container_root.as_os_str());
            fs::create_dir(container_root).expect("mkdir mntcont");
        }

        domount(None, Some(container_root), Some("tmpfs"), 0);
        domount(Some("none"), Some("/"), None, MS_REC|MS_PRIVATE);
        let proc_dir = container_root.join("proc");
        println!("setting up {:?}", proc_dir.as_os_str());
        fs::create_dir(proc_dir.as_path()).expect("mkdir proc");
        domount(None, Some(proc_dir.as_path()), Some("proc"), MS_NOSUID|MS_NODEV|MS_NOEXEC);

        // TODO populate root?

        //TODO pivot root

        {
            println!("mountinfo");
            let mut fstatus = File::open("/proc/self/mountinfo").expect("open mountinfo");
            let mut contents = String::new();
            fstatus.read_to_string(&mut contents).expect("read status");
            for l in contents.lines(){
                println!("{}", l);
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
