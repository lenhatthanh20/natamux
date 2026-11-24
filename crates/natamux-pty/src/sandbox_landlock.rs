//! Landlock through raw system calls (no crate): the ruleset and the folder handles are made in
//! the parent, the child only calls prctl(NO_NEW_PRIVS) and landlock_restrict_self. The rights
//! are masked to the ABI the kernel has, so an older kernel gets every right it knows.

use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use nix::libc;

use crate::sandbox_plan::IsolationPlan;

// The same numbers on every architecture (the shared syscall table).
const SYS_CREATE_RULESET: libc::c_long = 444;
const SYS_ADD_RULE: libc::c_long = 445;
const SYS_RESTRICT_SELF: libc::c_long = 446;
const CREATE_RULESET_VERSION: libc::c_long = 1;
const RULE_PATH_BENEATH: libc::c_long = 1;
const RULE_NET_PORT: libc::c_long = 2;
const CONNECT_TCP: u64 = 1 << 1;
const SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1 << 0;
const SCOPE_SIGNAL: u64 = 1 << 1;
const NO_FLAGS: libc::c_long = 0;
const ON: libc::c_ulong = 1;
const UNUSED: libc::c_ulong = 0;

const EXECUTE: u64 = 1 << 0;
const WRITE_FILE: u64 = 1 << 1;
const READ_FILE: u64 = 1 << 2;
const READ_DIR: u64 = 1 << 3;
/// ABI 1 handles bits 0 to 12.
const ABI_1: u64 = (1 << 13) - 1;
const REFER: u64 = 1 << 13;
const TRUNCATE: u64 = 1 << 14;
const IOCTL_DEV: u64 = 1 << 15;
/// The only rights a rule on a file (not a folder) may name.
const FILE_RIGHTS: u64 = EXECUTE | WRITE_FILE | READ_FILE | TRUNCATE | IOCTL_DEV;
const READ_RIGHTS: u64 = EXECUTE | READ_FILE | READ_DIR;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    /// ABI 4: the size passed stops before this field when TCP is not handled.
    handled_access_net: u64,
    /// ABI 6: the size passed stops before this field on an older kernel.
    scoped: u64,
}

#[repr(C)]
struct NetPortAttr {
    allowed_access: u64,
    port: u64,
}

#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

pub struct Ruleset(OwnedFd);

impl Ruleset {
    pub fn raw(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

/// The Landlock ABI of the running kernel. An error when Landlock is missing or turned off.
pub fn abi() -> io::Result<u32> {
    // SAFETY: a null attribute, size 0 and the version flag only return a number.
    let found = unsafe {
        libc::syscall(
            SYS_CREATE_RULESET,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            CREATE_RULESET_VERSION,
        )
    };
    if found < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(found as u32)
}

/// Every right the ABI knows. The rest stay allowed by the kernel, as it cannot check them.
pub fn handled(abi: u32) -> u64 {
    let mut rights = ABI_1;
    if abi >= 2 {
        rights |= REFER;
    }
    if abi >= 3 {
        rights |= TRUNCATE;
    }
    if abi >= 5 {
        rights |= IOCTL_DEV;
    }
    rights
}

/// The ruleset of a plan. Any error (no Landlock, ABI 0, a path that cannot be opened) fails the
/// spawn: a half-applied plan would look isolated and not be.
pub fn build(plan: &IsolationPlan) -> io::Result<Ruleset> {
    let kernel = abi()?;
    let abi = plan.max_abi.map_or(kernel, |max| max.min(kernel));
    if abi < 1 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Landlock ABI 0: isolation cannot run",
        ));
    }
    let handled = handled(abi);
    let tcp = plan.tcp_connect.as_deref().filter(|_| abi >= 4);
    // Signals and abstract unix sockets stay inside the sandbox; the daemon can still signal in.
    let scoped = abi >= 6;
    let attr = RulesetAttr {
        handled_access_fs: handled,
        handled_access_net: if tcp.is_some() { CONNECT_TCP } else { 0 },
        scoped: if scoped {
            SCOPE_ABSTRACT_UNIX_SOCKET | SCOPE_SIGNAL
        } else {
            0
        },
    };
    // An older kernel knows only the fields before the ones it has no ABI for.
    let size = if scoped {
        std::mem::size_of::<RulesetAttr>()
    } else if tcp.is_some() {
        std::mem::offset_of!(RulesetAttr, scoped)
    } else {
        std::mem::offset_of!(RulesetAttr, handled_access_net)
    };
    // SAFETY: the attribute lives for the call and its size is given.
    let fd = unsafe {
        libc::syscall(
            SYS_CREATE_RULESET,
            &attr as *const RulesetAttr,
            size,
            NO_FLAGS,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the kernel gave a new file descriptor that nothing else owns (close-on-exec).
    let ruleset = unsafe { OwnedFd::from_raw_fd(fd as RawFd) };
    for path in &plan.read_only {
        add(&ruleset, path, handled & READ_RIGHTS)?;
    }
    for path in &plan.read_write {
        add(&ruleset, path, handled)?;
    }
    for port in tcp.unwrap_or_default() {
        add_port(&ruleset, *port)?;
    }
    Ok(Ruleset(ruleset))
}

fn add(ruleset: &OwnedFd, path: &Path, access: u64) -> io::Result<()> {
    let named =
        |error: io::Error| io::Error::new(error.kind(), format!("{}: {error}", path.display()));
    let text = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| named(io::Error::from(io::ErrorKind::InvalidInput)))?;
    // SAFETY: a valid C string; the descriptor is owned right after.
    let fd = unsafe { libc::open(text.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(named(io::Error::last_os_error()));
    }
    // SAFETY: open gave a new descriptor that nothing else owns.
    let file = File::from(unsafe { OwnedFd::from_raw_fd(fd) });
    let is_dir = file.metadata().map_err(named)?.is_dir();
    let access = if is_dir { access } else { access & FILE_RIGHTS };
    let attr = PathBeneathAttr {
        allowed_access: access,
        parent_fd: file.as_raw_fd(),
    };
    // SAFETY: the attribute lives for the call; the ruleset and the path descriptor are open.
    let done = unsafe {
        libc::syscall(
            SYS_ADD_RULE,
            libc::c_long::from(ruleset.as_raw_fd()),
            RULE_PATH_BENEATH,
            &attr as *const PathBeneathAttr,
            NO_FLAGS,
        )
    };
    if done < 0 {
        return Err(named(io::Error::last_os_error()));
    }
    Ok(())
}

fn add_port(ruleset: &OwnedFd, port: u16) -> io::Result<()> {
    let attr = NetPortAttr {
        allowed_access: CONNECT_TCP,
        port: u64::from(port),
    };
    // SAFETY: the attribute lives for the call; the ruleset is open.
    let done = unsafe {
        libc::syscall(
            SYS_ADD_RULE,
            libc::c_long::from(ruleset.as_raw_fd()),
            RULE_NET_PORT,
            &attr as *const NetPortAttr,
            NO_FLAGS,
        )
    };
    if done < 0 {
        let error = io::Error::last_os_error();
        return Err(io::Error::new(
            error.kind(),
            format!("TCP port {port}: {error}"),
        ));
    }
    Ok(())
}

/// In the child, between fork and exec: only async-signal-safe calls. NO_NEW_PRIVS is what lets
/// a process without root turn Landlock on; it also keeps setuid programs such as sudo from
/// gaining rights.
pub fn restrict(ruleset: RawFd) -> io::Result<()> {
    // SAFETY: prctl with integer arguments.
    let set = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, ON, UNUSED, UNUSED, UNUSED) };
    if set != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a system call with integer arguments.
    let done = unsafe { libc::syscall(SYS_RESTRICT_SELF, libc::c_long::from(ruleset), NO_FLAGS) };
    if done != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_abi_handles_the_rights_it_knows() {
        assert_eq!(handled(1), ABI_1);
        assert_eq!(handled(2), ABI_1 | REFER);
        assert_eq!(handled(4), ABI_1 | REFER | TRUNCATE);
        assert_eq!(handled(6), ABI_1 | REFER | TRUNCATE | IOCTL_DEV);
        assert_eq!(handled(1) & READ_RIGHTS, READ_RIGHTS);
        assert_eq!(FILE_RIGHTS & READ_DIR, 0);
    }

    #[test]
    fn the_packed_rule_has_the_size_the_kernel_reads() {
        assert_eq!(std::mem::size_of::<PathBeneathAttr>(), 12);
        assert_eq!(std::mem::size_of::<RulesetAttr>(), 24);
        assert_eq!(std::mem::offset_of!(RulesetAttr, handled_access_net), 8);
        assert_eq!(std::mem::offset_of!(RulesetAttr, scoped), 16);
        assert_eq!(std::mem::size_of::<NetPortAttr>(), 16);
    }
}
