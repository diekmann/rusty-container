extern crate libc;

use libc::{c_void, c_int, c_ulong, c_char};
use std::ptr;
use std::path::Path;
use std::ffi::{CString, CStr};
use std::os::unix::ffi::OsStrExt;


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
pub fn pivot_root<P: AsRef<Path>>(new_root: P, put_old: P) {
    let new_root = p2cstr(new_root).as_ptr();
    let put_old = p2cstr(put_old).as_ptr();
    let r = unsafe { ffi::pivot_root(new_root, put_old) };
    assert_eq!(r, 0);
}

pub fn mount<P: AsRef<Path>>(source: Option<P>, target: Option<P>, filesystemtype: Option<&str>, mountflags: c_ulong) -> () {
    let source = source.map(p2cstr);
    let target = target.map(p2cstr);
    let fstype = match filesystemtype {
            None => ptr::null(),
            Some(fst) => CString::new(fst).expect("fstypes has NULLs").as_ptr(),
        };
    let r = unsafe { libc::mount(cstr2p(source), cstr2p(target), fstype, mountflags, ptr::null()) };
    assert_eq!(r, 0);
}

pub fn umount2<P: AsRef<Path>>(target: P, flags: c_int) {
    let target = p2cstr(target);
    assert_eq!( unsafe { libc::umount2(target.as_ptr(), flags) }, 0);
}
