extern crate libc;
extern crate contain;

use libc::{c_void, c_int, c_ulong, c_char};
use std::ptr;
use libc::{MS_REC, MS_PRIVATE, MS_NOSUID, MS_NODEV, MS_NOEXEC, MNT_DETACH};
use libc::{CLONE_NEWUSER, CLONE_NEWNS, CLONE_NEWPID, SIGCHLD};
use std::fs;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::io::{Read, Write};
use std::thread;
use std::ffi::{CString, CStr};
use std::os::unix::ffi::OsStrExt;
use std::process::Command;
use contain::Stack;


fn has_busybox() -> Option<String> {
    let o = Command::new("which")
            .arg("busybox")
            .output().expect("failed to start `which' program");
    if o.stderr.len() != 0 {
        println!("{}", String::from_utf8_lossy(&o.stderr));
    }
    if !o.status.success() {
        return None
    }
    //strip newline
    let path = {
        let p = String::from_utf8(o.stdout).unwrap();
        let mut l = p.lines();
        let path = String::from(l.next().unwrap());
        assert_eq!(l.next(), None);
        path
    };

    //check that is statically linked
    let dynamic_cmd = format!("test -z \"$(objdump -p {} | grep NEEDED)\"", path);
    println!("{}", dynamic_cmd);
    let o = Command::new("sh").arg("-c").arg(dynamic_cmd).status().expect("failed to check whether busybox is static linked");
    if !o.success() {
        println!("busybox found at {} but it does not seem to be a static executable", path);
        None
    } else {
        Some(path)
    }

}


//path to cstr
fn p2cstr<P: AsRef<Path>>(p: P) -> CString {
    CString::new(p.as_ref().as_os_str().as_bytes()).expect("string contains NULLs")
}

// CString Option to pointer
fn cstr2p(o: Option<CString>) -> *const c_char {
    o.map_or(ptr::null(), |x| x.as_ptr())
}


mod ffi{
    use libc::{c_int, c_char};
    extern {
        pub fn pivot_root(new_root: *const c_char, put_old: *const c_char) -> c_int;
    }
}
fn pivot_root<P: AsRef<Path>>(new_root: P, put_old: P) {
    let new_root = p2cstr(new_root).as_ptr();
    let put_old = p2cstr(put_old).as_ptr();
    let r = unsafe { ffi::pivot_root(new_root, put_old) };
    assert_eq!(r, 0);
}

fn domount<P: AsRef<Path>>(source: Option<P>, target: Option<P>, filesystemtype: Option<&str>, mountflags: c_ulong) -> () {
    let source = source.map(p2cstr);
    let target = target.map(p2cstr);
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

        println!("mounting container root");
        domount(None, Some(container_root), Some("tmpfs"), 0);
        domount(Some("none"), Some("/"), None, MS_REC|MS_PRIVATE);
        {
            let proc_dir = container_root.join("proc");
            println!("setting up {:?}", proc_dir.as_os_str());
            fs::create_dir(proc_dir.as_path()).expect("mkdir proc");
            domount(None, Some(proc_dir.as_path()), Some("proc"), MS_NOSUID|MS_NODEV|MS_NOEXEC);
        }

        // TODO populate root. copy busybox to it
        match has_busybox() {
            None => println!("not populating root (no busybox found)"),
            Some(busyboxpath) => {
                println!("populating root with a busybox image.");
                assert!(fs::copy(busyboxpath, container_root.join("busybox")).expect("copy busybox") > 0);
            }
        }

        let old_root_name = "oldroot";
        {
            println!("Entering new root");
            let old_root = container_root.join(old_root_name);
            fs::create_dir(old_root.as_path()).expect("mkdir oldroot");
            pivot_root(container_root, old_root.as_path());
        }

        unsafe { let root = CString::new("/").unwrap();
            assert_eq!(libc::chdir(root.as_ptr()), 0);
        };

        // we moved to the new root
        let container_root = Path::new("/");
        let old_root = container_root.join(old_root_name);

        {
            println!("getting rid of old root");
            assert_eq!( unsafe { libc::umount2(p2cstr(old_root.as_path()).as_ptr(), MNT_DETACH) }, 0);
            assert!(fs::read_dir(old_root.as_path()).expect("oldroot dir").next().is_none());
            fs::remove_dir(old_root.as_path()).expect("rmdir oldroot");
        }

        {
            println!("mountinfo");
            let mut fstatus = File::open("/proc/self/mountinfo").expect("open mountinfo");
            let mut contents = String::new();
            fstatus.read_to_string(&mut contents).expect("read status");
            for l in contents.lines(){
                println!("{}", l);
            }
        }
        0
    });

    match h.join() {
        Ok(_) => {
            let prog = CString::new("/busybox").unwrap();
            let arg0 = CString::new("busybox").unwrap();
            let arg1 = CString::new("sh").unwrap();
            let argv = vec![arg0.as_ptr(), arg1.as_ptr(), ptr::null()];
            unsafe { libc::execv(prog.as_ptr(), argv.as_ptr()) };
            panic!("exec failed!"); //stack unwinding will also panic :D
        }
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
