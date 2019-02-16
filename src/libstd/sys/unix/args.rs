//! Global initialization and retrieval of command line arguments.
//!
//! On some platforms these are stored during runtime startup,
//! and on some they are retrieved from the system on demand.

#![allow(dead_code)] // runtime init functions not used during testing

use ffi::{OsString,OsStr,CStr};
use marker::PhantomData;
use os::unix::prelude::{OsStringExt,OsStrExt};

/// One-time global initialization.
pub unsafe fn init(argc: isize, argv: *const *const u8) { imp::init(argc, argv) }

/// One-time global cleanup.
pub unsafe fn cleanup() { imp::cleanup() }

/// Returns the command line arguments
pub fn args() -> Args {
    imp::args()
}

pub struct Args {
    argc: isize,
    argv: *const *const u8,
    _dont_send_or_sync_me: PhantomData<*mut ()>,
}

impl Args {
    pub fn inner_debug(&self) -> Vec<&OsStr> {
        self.as_refs().collect()
    }

    pub fn as_refs(&self) -> ArgsRefs {
        ArgsRefs {
            argc : self.argc,
            argv : self.argv,
            _dont_send_or_sync_me: PhantomData
        }
    }
}

impl Iterator for Args {
    type Item = OsString;
    fn next(&mut self) -> Option<OsString> {
        if self.argc != 0 {
            unsafe {
                let cstr = CStr::from_ptr(self.argv.read() as *const libc::c_char);
                let rv = OsStringExt::from_vec(cstr.to_bytes().to_vec());
                self.argc -= 1;
                self.argv = self.argv.offset(1);
                Some(rv)
            }
        } else {
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) { (self.argc as usize, Some(self.argc as usize)) }
}

impl ExactSizeIterator for Args {
    fn len(&self) -> usize { self.argc as usize }
}

impl DoubleEndedIterator for Args {
    fn next_back(&mut self) -> Option<OsString> {
        if self.argc != 0 {
            self.argc -= 1;
            unsafe {
                let cstr = CStr::from_ptr(*self.argv.offset(self.argc) as *const libc::c_char);
                Some(OsStringExt::from_vec(cstr.to_bytes().to_vec()))
            }
        } else {
            None
        }
    }
}

pub struct ArgsRefs<'a> {
    argc: isize,
    argv: *const *const u8,
    _dont_send_or_sync_me: PhantomData<&'a mut *mut ()>,
}

impl<'a> Iterator for ArgsRefs<'a> {
    type Item = &'a OsStr;
    fn next(&mut self) -> Option<&'a OsStr> {
        if self.argc != 0 {
            unsafe {
                let cstr = CStr::from_ptr(self.argv.read() as *const libc::c_char);
                let rv = OsStrExt::from_bytes(cstr.to_bytes());
                self.argc -= 1;
                self.argv = self.argv.offset(1);
                Some(rv)
            }
        } else {
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) { (self.argc as usize, Some(self.argc as usize)) }
}

impl<'a> ExactSizeIterator for ArgsRefs<'a> {
    fn len(&self) -> usize { self.argc as usize }
}

impl<'a> DoubleEndedIterator for ArgsRefs<'a> {
    fn next_back(&mut self) -> Option<&'a OsStr> {
        if self.argc != 0 {
            self.argc -= 1;
            unsafe {
                let cstr = CStr::from_ptr(*self.argv.offset(self.argc) as *const libc::c_char);
                Some(OsStrExt::from_bytes(cstr.to_bytes()))
            }
        } else {
            None
        }
    }
}

#[cfg(any(target_os = "linux",
          target_os = "android",
          target_os = "freebsd",
          target_os = "dragonfly",
          target_os = "bitrig",
          target_os = "netbsd",
          target_os = "openbsd",
          target_os = "solaris",
          target_os = "emscripten",
          target_os = "haiku",
          target_os = "l4re",
          target_os = "fuchsia",
          target_os = "hermit"))]
mod imp {
    use ptr;
    use marker::PhantomData;
    use super::Args;

    use sys_common::mutex::Mutex;

    static mut ARGC: isize = 0;
    static mut ARGV: *const *const u8 = ptr::null();
    // We never call `ENV_LOCK.init()`, so it is UB to attempt to
    // acquire this mutex reentrantly!
    static LOCK: Mutex = Mutex::new();

    pub unsafe fn init(argc: isize, argv: *const *const u8) {
        let _guard = LOCK.lock();
        ARGC = argc;
        ARGV = argv;
    }

    pub unsafe fn cleanup() {
        let _guard = LOCK.lock();
        ARGC = 0;
        ARGV = ptr::null();
    }

    pub fn args() -> Args {
        unsafe {
            let _guard = LOCK.lock();
            Args {
                argc : ARGC,
                argv : ARGV,
                _dont_send_or_sync_me: PhantomData
            }
        }
    }
}

#[cfg(any(target_os = "macos",
          target_os = "ios"))]
mod imp {
    use ffi::CStr;
    use marker::PhantomData;
    use libc;
    use super::Args;

    pub unsafe fn init(_argc: isize, _argv: *const *const u8) {
    }

    pub fn cleanup() {
    }

    #[cfg(target_os = "macos")]
    pub fn args() -> Args {
        use os::unix::prelude::*;
        extern {
            // These functions are in crt_externs.h.
            fn _NSGetArgc() -> *mut libc::c_int;
            fn _NSGetArgv() -> *mut *mut *mut libc::c_char;
        }

        let vec = unsafe {
            let (argc, argv) = (*_NSGetArgc() as isize,
                                *_NSGetArgv() as *const *const libc::c_char);
            (0.. argc as isize).map(|i| {
                let bytes = CStr::from_ptr(*argv.offset(i)).to_bytes().to_vec();
                OsStringExt::from_vec(bytes)
            }).collect::<Vec<_>>()
        };
        Args {
            iter: vec.into_iter(),
            _dont_send_or_sync_me: PhantomData,
        }
    }

    // As _NSGetArgc and _NSGetArgv aren't mentioned in iOS docs
    // and use underscores in their names - they're most probably
    // are considered private and therefore should be avoided
    // Here is another way to get arguments using Objective C
    // runtime
    //
    // In general it looks like:
    // res = Vec::new()
    // let args = [[NSProcessInfo processInfo] arguments]
    // for i in (0..[args count])
    //      res.push([args objectAtIndex:i])
    // res
    #[cfg(target_os = "ios")]
    pub fn args() -> Args {
        use ffi::OsString;
        use mem;
        use str;

        extern {
            fn sel_registerName(name: *const libc::c_uchar) -> Sel;
            fn objc_getClass(class_name: *const libc::c_uchar) -> NsId;
        }

        #[cfg(target_arch="aarch64")]
        extern {
            fn objc_msgSend(obj: NsId, sel: Sel) -> NsId;
            #[link_name="objc_msgSend"]
            fn objc_msgSend_ul(obj: NsId, sel: Sel, i: libc::c_ulong) -> NsId;
        }

        #[cfg(not(target_arch="aarch64"))]
        extern {
            fn objc_msgSend(obj: NsId, sel: Sel, ...) -> NsId;
            #[link_name="objc_msgSend"]
            fn objc_msgSend_ul(obj: NsId, sel: Sel, ...) -> NsId;
        }

        type Sel = *const libc::c_void;
        type NsId = *const libc::c_void;

        let mut res = Vec::new();

        unsafe {
            let process_info_sel = sel_registerName("processInfo\0".as_ptr());
            let arguments_sel = sel_registerName("arguments\0".as_ptr());
            let utf8_sel = sel_registerName("UTF8String\0".as_ptr());
            let count_sel = sel_registerName("count\0".as_ptr());
            let object_at_sel = sel_registerName("objectAtIndex:\0".as_ptr());

            let klass = objc_getClass("NSProcessInfo\0".as_ptr());
            let info = objc_msgSend(klass, process_info_sel);
            let args = objc_msgSend(info, arguments_sel);

            let cnt: usize = mem::transmute(objc_msgSend(args, count_sel));
            for i in 0..cnt {
                let tmp = objc_msgSend_ul(args, object_at_sel, i as libc::c_ulong);
                let utf_c_str: *const libc::c_char =
                    mem::transmute(objc_msgSend(tmp, utf8_sel));
                let bytes = CStr::from_ptr(utf_c_str).to_bytes();
                res.push(OsString::from(str::from_utf8(bytes).unwrap()))
            }
        }

        Args { iter: res.into_iter(), _dont_send_or_sync_me: PhantomData }
    }
}
