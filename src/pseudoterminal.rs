// TODO: This is crappy and unsafe is used without care. Ideally we should use
// safe bindings.

use std::ffi::{c_int, CStr};
use std::fs::File;
use std::os::fd::FromRawFd;

unsafe fn perror(msg: &CStr) {
  libc::perror(msg.as_ptr());
}

unsafe fn ptsname(mfd: c_int) -> Option<&'static CStr> {
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
  let pid = unsafe { libc::getpid() };
  println!("My PID is {pid}");

  let (mfd, name) = unsafe { create_pty_master() };
  println!("Pseudoterminal created: {name}");

  let fork_result = unsafe { libc::fork() };

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

unsafe fn create_pty_master() -> (c_int, String) {
  let res = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
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

  let res = libc::unlockpt(mfd);
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
unsafe fn connect_to_pty_slave(mfd: i32) {
  // Close mfd
  assert!(libc::setsid() != -1);
  let name = ptsname(mfd);
  let sfd = libc::open(name.unwrap().as_ptr(), 0);
  println!(
    "Slave opened, name: {}, descriptor: {}",
    name.unwrap().to_string_lossy(), sfd
  );
  libc::dup2(sfd, 0);
  libc::dup2(sfd, 1);
  libc::dup2(sfd, 2);
  println!(
    "Ready to run child..."
  );
}
