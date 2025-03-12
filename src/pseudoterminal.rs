// TODO: This is crappy and unsafe is used without care. Replace with the clib
// crate.

use std::ffi::{c_char, c_int, CStr};
use std::fs::File;
use std::os::fd::FromRawFd;

extern "C" {
  fn fork() -> c_int;
  /// Returns the process ID
  fn getpid() -> c_int;

  /// Finds and opens an unused pseudoterminal master device, and returns a file
  /// descriptor that can later be used to refer to this device.
  fn posix_openpt(flags: c_int) -> c_int;
  // fn ptsname(mfd: c_int) -> *const c_char;
  /// Devuelve 0 en caso de éxito, -1 en otro caso
  fn unlockpt(mfd: c_int) -> c_int;


  fn dup2(a: c_int, b: c_int) -> c_int;
  fn open(path: *const c_char) -> c_int;
  fn setsid() -> c_int;
}

const O_RDWR: i32   = 0x0002; // 00000002;
const O_NOCTTY: i32 = 0x0100; // 00000400;

mod libc {
  use std::ffi::{c_void, c_int, c_char};
  extern "C" {
    pub fn perror(s: *const c_char) -> c_void;
    pub fn ptsname(mfd: c_int) -> *const c_char;
  }
}

fn perror(msg: &CStr) {
  unsafe { libc::perror(msg.as_ptr()); }
}

/// Hmmm, el bufer podría ser problemático
fn ptsname(mfd: c_int) -> Option<&'static CStr> {
  // https://www.man7.org/linux/man-pages/man3/ptsname.3.html
  let result = unsafe { libc::ptsname(mfd) };
  match result {
    ptr if ptr.is_null() => None,
    ptr => Some(unsafe { CStr::from_ptr(ptr) })
  }
}

pub fn run_in_pseudoterminal(
  master_code: impl FnOnce(File),
  slave_code: impl FnOnce()
) {
  let pid = unsafe { getpid() };
  println!("My PID is {pid}");

  let (mfd, name) = unsafe { create_pty_master() };
  println!("Pseudoterminal created: {name}");

  let fork_result = unsafe { fork() };

  match fork_result {

    // Parent
    result if result > 0 => {
      let master_file_descriptor = unsafe { std::fs::File::from_raw_fd(mfd) };
      master_code(master_file_descriptor);
    }

    // Child
    0 => {
      unsafe { connect_to_pty_slave(mfd); }
      slave_code()
    }

    result => {
      panic!("Fork failed with code {result}")
    }
  }
}

pub unsafe fn create_pty_master() -> (c_int, String) {
  let res = posix_openpt(O_RDWR | O_NOCTTY);
  let mfd = match res {
    mfd if mfd > 0 => {
      println!("We have a pseudoterminal at fd {mfd}");
      mfd
    },
    _ => {
      perror(c"Failed to open a pseudo terminal");
      panic!("Failed to open a pseudo terminal");
    }
  };

  let res = unlockpt(mfd);
  if res != 0 {
    perror(c"Failed to unlock the pseudoterminal");
    panic!("Failed to unlock the pseudoterminal");
  }

  if let Some(name) = ptsname(mfd) {
    let name = name.to_string_lossy().to_string();
    println!("The name of the slave is {name}");
    (mfd, name)
  } else {
    panic!("Failed to get the slave name");
  }
}

/// Conecta el proceso actual al extremo esclavo de pseudoterminal, de modo que
/// recibe su input y manda su output al maestro.
pub unsafe fn connect_to_pty_slave(mfd: i32) {
  // Close mfd
  assert!(setsid() != -1);
  let name = ptsname(mfd);
  let sfd = open(name.unwrap().as_ptr());
  println!(
    "Slave opened, name: {}, descriptor: {}",
    name.unwrap().to_string_lossy(), sfd
  );
  dup2(sfd, 0);
  dup2(sfd, 1);
  dup2(sfd, 2);
  println!(
    "Ready to run child..."
  );
}
