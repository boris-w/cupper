//! Process facts from libproc and `sysctl`: start time, parent chain, session
//! and process group (design 5.6.8, 5.8.4).

use std::{mem, ptr};

use libc::c_char;

/// When a process started, in microseconds since the Unix epoch. A pid plus its
/// start time names one process: a recycled pid comes with a later start time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StartTime(u64);

impl StartTime {
    pub const fn from_micros(micros: u64) -> Self {
        Self(micros)
    }

    pub const fn as_micros(self) -> u64 {
        self.0
    }
}

/// A live (not zombie) process as the kernel reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: i32,
    pub ppid: i32,
    pub pgid: i32,
    pub sid: i32,
    pub uid: u32,
    pub start: StartTime,
    /// The kernel's short process name (at most 16 bytes), for display only.
    pub name: String,
}

/// `p_stat` of a process that has exited but not been waited for (`<sys/proc.h>`).
const SZOMB: u32 = 5;

/// Longest parent chain followed; real chains are a handful of processes deep.
const MAX_CHAIN: usize = 128;

/// The kernel's short BSD record of `pid`, zombies included. Unlike the full
/// record (`PROC_PIDTBSDINFO`), it can be read for processes of other users,
/// which parent chains pass through (`login`, `launchd`).
fn short_info(pid: i32) -> Option<libc::proc_bsdshortinfo> {
    if pid <= 0 {
        return None;
    }
    // SAFETY: proc_bsdshortinfo holds only integers and C char arrays, for
    // which all-zero bytes are a valid value.
    let mut info: libc::proc_bsdshortinfo = unsafe { mem::zeroed() };
    let size = mem::size_of::<libc::proc_bsdshortinfo>() as libc::c_int;
    // SAFETY: the buffer is one proc_bsdshortinfo of `size` bytes and the
    // kernel writes at most `size` bytes into it.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDT_SHORTBSDINFO,
            0,
            (&raw mut info).cast(),
            size,
        )
    };
    (written == size).then_some(info)
}

/// `sizeof(struct kinfo_proc)` on 64-bit macOS (`<sys/sysctl.h>`).
const KINFO_PROC_SIZE: usize = 648;

/// `struct kinfo_proc`. Only its first field is read: `struct extern_proc`
/// starts with a union whose `__p_starttime` arm is the start time.
#[repr(C)]
struct KinfoProc {
    p_starttime: libc::timeval,
    _rest: [u8; KINFO_PROC_SIZE - mem::size_of::<libc::timeval>()],
}

const _: () = assert!(mem::size_of::<KinfoProc>() == KINFO_PROC_SIZE);

/// When `pid` started, zombies included. Read with `sysctl(KERN_PROC_PID)`,
/// which, unlike the full libproc record, also works for other users'
/// processes.
pub(crate) fn start_time(pid: i32) -> Option<StartTime> {
    if pid <= 0 {
        return None;
    }
    let mut mib = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid];
    // SAFETY: KinfoProc holds only integers and byte arrays, for which
    // all-zero bytes are a valid value.
    let mut kinfo: KinfoProc = unsafe { mem::zeroed() };
    let mut len = mem::size_of::<KinfoProc>();
    // SAFETY: `mib` holds the 4 names passed; the buffer is one KinfoProc of
    // `len` bytes and the kernel writes at most `len` bytes into it, storing
    // the written length back (0 when there is no such process).
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            (&raw mut kinfo).cast(),
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len != KINFO_PROC_SIZE {
        return None;
    }
    let tv = kinfo.p_starttime;
    let secs = u64::try_from(tv.tv_sec).ok()?;
    let micros = u64::try_from(tv.tv_usec).ok()?;
    Some(StartTime(
        secs.saturating_mul(1_000_000).saturating_add(micros),
    ))
}

/// Facts about `pid`; `None` when it does not exist, is a zombie, or cannot be
/// read.
pub fn process_info(pid: i32) -> Option<ProcessInfo> {
    let info = short_info(pid)?;
    if info.pbsi_status == SZOMB {
        return None;
    }
    let start = start_time(pid)?;
    // SAFETY: getsid takes a plain pid and touches no memory of ours.
    let sid = unsafe { libc::getsid(pid) };
    if sid < 0 {
        return None;
    }
    Some(ProcessInfo {
        pid,
        ppid: info.pbsi_ppid as i32,
        pgid: info.pbsi_pgid as i32,
        sid,
        uid: info.pbsi_uid,
        start,
        name: c_chars(&info.pbsi_comm),
    })
}

fn c_chars(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `pid` followed by its ancestors, nearest first, up to launchd (pid 1).
///
/// The chain stops early where an ancestor cannot be read or started after its
/// child (its pid was recycled while we walked); a chain that does not end at
/// pid 1 is therefore broken (design 5.8.4: "cannot confirm the origin").
pub fn parent_chain(pid: i32) -> Vec<ProcessInfo> {
    let mut chain: Vec<ProcessInfo> = Vec::new();
    let mut next = pid;
    while chain.len() < MAX_CHAIN {
        let Some(info) = process_info(next) else {
            break;
        };
        if chain.last().is_some_and(|child| info.start > child.start) {
            break;
        }
        let ppid = info.ppid;
        let top = info.pid == 1 || ppid <= 0 || ppid == info.pid;
        chain.push(info);
        if top {
            break;
        }
        next = ppid;
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_info_describes_this_process() {
        let me = process_info(std::process::id() as i32).unwrap();
        assert_eq!(me.ppid, std::os::unix::process::parent_id() as i32);
        assert_eq!(me.uid, crate::current_uid());
        assert!(me.pgid > 0 && me.sid > 0);
        assert!(me.start.as_micros() > 0);
        assert!(!me.name.is_empty());
        let again = process_info(me.pid).unwrap();
        assert_eq!(again.start, me.start);
    }

    #[test]
    fn process_info_of_a_missing_pid_is_none() {
        assert_eq!(process_info(0), None);
        assert_eq!(process_info(-1), None);
        assert_eq!(process_info(i32::MAX), None);
    }

    #[test]
    fn parent_chain_of_this_process_ends_at_launchd() {
        let me = std::process::id() as i32;
        let chain = parent_chain(me);
        assert_eq!(chain[0].pid, me);
        assert_eq!(chain[1].pid, std::os::unix::process::parent_id() as i32);
        let top = chain.last().unwrap();
        assert_eq!((top.pid, top.name.as_str()), (1, "launchd"));
        for pair in chain.windows(2) {
            assert_eq!(pair[0].ppid, pair[1].pid);
            assert!(pair[1].start <= pair[0].start);
        }
    }
}
