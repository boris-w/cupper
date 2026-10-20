//! Process facts from libproc and `sysctl`: start time, parent chain, session
//! and process group (design 5.6.8, 5.8.4).

use std::collections::{HashSet, VecDeque};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::{mem, ptr};

use libc::c_char;

use crate::{Error, Result};

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

/// Longest parent chain followed; real chains are a handful of processes deep.
const MAX_CHAIN: usize = 128;

/// The kernel's short BSD record of `pid`. Unlike the full record
/// (`PROC_PIDTBSDINFO`), it can be read for processes of other users, which
/// parent chains pass through (`login`, `launchd`). Neither can be read for a
/// zombie.
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

/// `PROC_PIDUNIQIDENTIFIERINFO` from XNU's private `<sys/proc_info_private.h>`.
const PROC_PIDUNIQIDENTIFIERINFO: libc::c_int = 17;

/// `struct proc_uniqidentifierinfo` from the same header; the libc crate does
/// not declare it.
#[repr(C)]
struct ProcUniqIdentifierInfo {
    _p_uuid: [u8; 16],
    p_uniqueid: u64,
    p_puniqueid: u64,
    p_idversion: i32,
    _p_reserve2: u32,
    _p_reserve3: u64,
    _p_reserve4: u64,
}

const _: () = assert!(mem::size_of::<ProcUniqIdentifierInfo>() == 56);

/// Identifiers of a process that, unlike its pid, the kernel does not reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UniqueIds {
    /// This process, for the whole boot; it is kept across exec.
    pub unique_id: u64,
    /// The process that started this one: the parent at its latest fork or
    /// exec. When the parent exits and launchd adopts the process, it stays,
    /// so a process can name its launcher without racing the launcher's exit
    /// (design 5.8.1: `dev_key` is only accepted from the launcher). Only an
    /// exec after the adoption makes it launchd's id. It never names another
    /// process.
    pub parent_unique_id: u64,
    /// Changes on every fork and exec; it is the pid version of the
    /// process's audit token.
    pub pid_version: u32,
}

/// The unique ids of `pid`; `None` when it does not exist, has exited, or
/// cannot be read. Other users' processes can be read too.
pub fn unique_ids(pid: i32) -> Option<UniqueIds> {
    if pid <= 0 {
        return None;
    }
    // SAFETY: the struct holds only integers and a byte array, for which
    // all-zero bytes are a valid value.
    let mut info: ProcUniqIdentifierInfo = unsafe { mem::zeroed() };
    let size = mem::size_of::<ProcUniqIdentifierInfo>() as libc::c_int;
    // SAFETY: the buffer is one struct of `size` bytes and the kernel writes
    // at most `size` bytes into it.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            PROC_PIDUNIQIDENTIFIERINFO,
            0,
            (&raw mut info).cast(),
            size,
        )
    };
    (written == size).then_some(UniqueIds {
        unique_id: info.p_uniqueid,
        parent_unique_id: info.p_puniqueid,
        pid_version: info.p_idversion as u32,
    })
}

/// Facts about `pid`; `None` when it does not exist, has exited (a zombie
/// waiting to be reaped), or cannot be read.
pub fn process_info(pid: i32) -> Option<ProcessInfo> {
    let info = short_info(pid)?;
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

/// The resident memory of `pid` in bytes (`PROC_PIDTASKINFO`); `None` when
/// it does not exist, has exited, or cannot be read.
pub fn resident_size(pid: i32) -> Option<u64> {
    if pid <= 0 {
        return None;
    }
    // SAFETY: the struct holds only integers, for which all-zero bytes are a
    // valid value.
    let mut info: libc::proc_taskinfo = unsafe { mem::zeroed() };
    let size = mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    // SAFETY: the buffer is one struct of `size` bytes and the kernel writes
    // at most `size` bytes into it.
    let written =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTASKINFO, 0, (&raw mut info).cast(), size) };
    (written == size).then_some(info.pti_resident_size)
}

/// The full path of the executable `pid` runs (`proc_pidpath`), with links
/// resolved.
pub fn pid_path(pid: i32) -> Result<PathBuf> {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: `buf` has the writable bytes its length says; the kernel writes
    // at most that many and returns how many.
    let len = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if len <= 0 {
        return Err(Error::last_os("proc_pidpath"));
    }
    buf.truncate(len as usize);
    Ok(PathBuf::from(OsString::from_vec(buf)))
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

/// Which pids [`list_pids`] lists.
#[derive(Clone, Copy)]
enum Listing {
    Children(i32),
    Group(i32),
}

/// The pids libproc lists, zombies included, grown until the buffer has room
/// to spare (a full buffer may have been cut short).
fn list_pids(listing: Listing) -> Vec<i32> {
    let mut capacity = 64;
    loop {
        let mut pids = vec![0 as libc::c_int; capacity];
        let bytes = libc::c_int::try_from(capacity * mem::size_of::<libc::c_int>())
            .expect("the process table is far smaller");
        // SAFETY: the buffer holds `bytes` bytes and the kernel writes at most
        // that many; both calls return the number of pids written, or -1.
        let count = unsafe {
            match listing {
                Listing::Children(ppid) => {
                    libc::proc_listchildpids(ppid, pids.as_mut_ptr().cast(), bytes)
                }
                Listing::Group(pgid) => {
                    libc::proc_listpgrppids(pgid, pids.as_mut_ptr().cast(), bytes)
                }
            }
        };
        let Ok(count) = usize::try_from(count) else {
            return Vec::new();
        };
        if count < capacity {
            pids.truncate(count);
            pids.retain(|&pid| pid > 0);
            return pids;
        }
        capacity *= 4;
    }
}

/// Live descendants of `pid`, breadth first, without `pid` itself. Once a
/// process has exited, its children belong to launchd and are no longer found
/// here (design 5.6.8).
pub fn descendants(pid: i32) -> Vec<ProcessInfo> {
    let mut found: Vec<ProcessInfo> = Vec::new();
    let mut seen = HashSet::from([pid]);
    let mut queue = VecDeque::from([pid]);
    while let Some(parent) = queue.pop_front() {
        for child in list_pids(Listing::Children(parent)) {
            if !seen.insert(child) {
                continue;
            }
            if let Some(info) = process_info(child).filter(|info| info.ppid == parent) {
                queue.push_back(child);
                found.push(info);
            }
        }
    }
    found
}

/// Live members of process group `pgid`, as they are now.
pub(crate) fn group_members(pgid: i32) -> Vec<ProcessInfo> {
    if pgid <= 0 {
        return Vec::new();
    }
    list_pids(Listing::Group(pgid))
        .into_iter()
        .filter_map(process_info)
        .filter(|info| info.pgid == pgid)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    use super::*;
    use crate::Command;
    use crate::testutil::{Groups, Owned, cat, eventually, sh};

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
    fn resident_size_of_this_process_and_of_none() {
        assert!(resident_size(std::process::id() as i32).unwrap() > 0);
        assert_eq!(resident_size(i32::MAX), None);
    }

    #[test]
    fn process_info_of_a_missing_pid_is_none() {
        assert_eq!(process_info(0), None);
        assert_eq!(process_info(-1), None);
        assert_eq!(process_info(i32::MAX), None);
    }

    #[test]
    fn process_info_of_a_zombie_is_none() {
        let mut child = Owned::spawn(&Command::new("/usr/bin/true"));
        assert!(eventually(Duration::from_secs(5), || {
            process_info(child.pid()).is_none()
        }));
        // Still ours to reap, so the pid has not been reused meanwhile.
        assert_eq!(start_time(child.pid()), Some(child.start()));
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn unit_sys_uniqueid_child_records_its_launcher() {
        let me = unique_ids(std::process::id() as i32).unwrap();
        let parent = unique_ids(std::os::unix::process::parent_id() as i32).unwrap();
        assert_eq!(me.parent_unique_id, parent.unique_id);

        // The host is started like this (SETSID) and reads its own record.
        let host = Owned::spawn(cat().new_session());
        let ids = unique_ids(host.pid()).unwrap();
        assert_eq!(ids.parent_unique_id, me.unique_id);
        assert!(ids.unique_id > me.unique_id, "{ids:?} {me:?}");
        assert_ne!(ids.pid_version, me.pid_version);

        // Another process of the same program started by the same launcher is
        // still told apart.
        let sibling = Owned::spawn(&cat());
        let other = unique_ids(sibling.pid()).unwrap();
        assert_eq!(other.parent_unique_id, me.unique_id);
        assert_ne!(other.unique_id, ids.unique_id);
    }

    #[test]
    fn unit_sys_uniqueid_exec_keeps_the_id_and_changes_the_pid_version() {
        let mut cmd = sh("read x && exec /bin/cat");
        cmd.stdin(crate::Stdio::Pipe);
        let mut child = Owned::spawn(&cmd);
        let mut stdin = child.stdin_file();
        let mut stdout = BufReader::new(child.stdout_file());
        let before = unique_ids(child.pid()).unwrap();
        stdin.write_all(b"exec\nping\n").unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        assert_eq!(line, "ping\n", "cat runs in place of sh");
        let after = unique_ids(child.pid()).unwrap();
        assert_eq!(after.unique_id, before.unique_id);
        assert_eq!(after.parent_unique_id, before.parent_unique_id);
        assert_ne!(after.pid_version, before.pid_version);
        drop(stdin);
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn unit_sys_uniqueid_parent_id_outlives_the_launcher_until_an_exec() {
        // sh starts a subshell in the background, reports its pid and waits
        // for us. The subshell reports once its trap is set, so that SIGUSR1
        // makes it exec sleep instead of killing it.
        let mut cmd = sh(
            "(trap 'exec /bin/sleep 30' USR1; echo ready; exec >/dev/null; \
             while :; do /bin/sleep 0.02; done) & \
             echo $!; read x || true",
        );
        cmd.stdin(crate::Stdio::Pipe);
        let mut shell = Owned::spawn(&cmd);
        let _groups = Groups(vec![shell.group()]);
        let stdin = shell.stdin_file();
        // The two lines may come in either order.
        let (mut child, mut ready) = (0, false);
        for line in BufReader::new(shell.stdout_file()).lines().take(2) {
            match line.unwrap().as_str() {
                "ready" => ready = true,
                pid => child = pid.parse().unwrap(),
            }
        }
        assert!(ready && child > 0);
        let launcher = unique_ids(shell.pid()).unwrap().unique_id;
        assert_eq!(unique_ids(child).unwrap().parent_unique_id, launcher);

        // Adoption by launchd keeps the record (macOS 26.4).
        drop(stdin);
        assert!(shell.wait().unwrap().success());
        assert!(eventually(Duration::from_secs(5), || {
            process_info(child).is_some_and(|info| info.ppid == 1)
        }));
        assert_eq!(unique_ids(child).unwrap().parent_unique_id, launcher);

        // An exec after the adoption records launchd instead.
        // SAFETY: kill takes plain integers; `child` is alive, in our group.
        assert_eq!(unsafe { libc::kill(child, libc::SIGUSR1) }, 0);
        assert!(eventually(Duration::from_secs(5), || {
            process_info(child).is_some_and(|info| info.name == "sleep")
        }));
        let launchd = unique_ids(1).unwrap().unique_id;
        assert_eq!(unique_ids(child).unwrap().parent_unique_id, launchd);
    }

    #[test]
    fn unit_sys_uniqueid_reads_root_processes_but_not_missing_ones() {
        let launchd = unique_ids(1).unwrap();
        assert!(launchd.unique_id > 0);
        assert_eq!(unique_ids(0), None);
        assert_eq!(unique_ids(-1), None);
        assert_eq!(unique_ids(i32::MAX), None);
    }

    #[test]
    fn unit_sys_uniqueid_of_a_zombie_is_none() {
        let mut child = Owned::spawn(&Command::new("/usr/bin/true"));
        assert!(eventually(Duration::from_secs(5), || {
            process_info(child.pid()).is_none()
        }));
        assert_eq!(unique_ids(child.pid()), None);
        assert!(child.wait().unwrap().success());
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
