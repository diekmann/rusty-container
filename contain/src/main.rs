extern crate contain;

use std::os::raw::c_int;
use std::fs;
use std::path::Path;
use std::process::Command;
use contain::linux;
use contain::runcontained;

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

fn setup(container_root: &Path) -> bool {
    // populate root. copy busybox to it
    match has_busybox() {
        None => {
            println!("not populating root (no busybox found)");
            false
        }
        Some(p) => {
            println!("populating root with a busybox image.");
            assert!(fs::copy(&p, container_root.join("busybox")).expect("copy busybox") > 0);
            true
        }
    }
}

fn run(busybox: bool) -> c_int {
    if busybox {
        linux::execv("/busybox", vec!["busybox", "sh"]);
        unreachable!()
    } else {
        println!("yolo");
        0
    }
}


fn main() {
    println!("Hello, world!");
    runcontained(setup, run);
}
