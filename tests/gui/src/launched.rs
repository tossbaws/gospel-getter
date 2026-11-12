//! The apps gui-launch starts on Windows, and what it records about each
//! (`GUI_LAUNCH_PIDS`), so the checks can make sure one has exited before
//! they start the next.
//!
//! Edge WebDriver ends a session by terminating the program it started,
//! which is gui-launch, not the app: the app is gui-launch's child, and on
//! Windows a child outlives its parent. So gui-launch puts itself, and with
//! it the app, in a job object that ends everything in it once gui-launch
//! is gone (see src/bin/gui-launch.rs). The checks don't take that on
//! trust. Each line of the record names one app by its pid, the moment it
//! was created and its program's full path: together they tell the app
//! from any process given the same pid later, so nothing but an app the
//! run itself started is ever waited for or stopped. The pid and the
//! creation time are enough on their own (a pid goes to another process
//! only once its process is gone, and that one is created later); the path
//! is checked as well whenever Windows says it, which it doesn't for a
//! process that is exiting ([`Identity::AppExiting`]). Windows refusing to
//! say ([`refused`]) is never taken as an answer that the process isn't the
//! app.

use anyhow::{Context, Result};

/// One app gui-launch started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    /// Its process id.
    pub pid: u32,
    /// When it was created (a Windows FILETIME, 100 ns since 1601).
    pub created: u64,
    /// Its program's full path, as Windows reports it.
    pub path: String,
}

/// What a running process that holds an app's pid is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identity {
    /// The app: the same pid, created at the same moment, running the same
    /// program.
    App,
    /// The app, by its pid and the moment it was created, which no other
    /// process can share; Windows no longer says what program it runs, as
    /// it doesn't once a process is exiting.
    AppExiting,
    /// Another process, created at another moment, holds the pid now.
    CreatedOtherwise,
    /// Created at the app's moment, but running another program: not the
    /// app.
    OtherProgram,
}

/// Windows' "Access is denied" (`ERROR_ACCESS_DENIED`).
pub const ERROR_ACCESS_DENIED: i32 = 5;

/// Windows' "A device attached to the system is not functioning"
/// (`ERROR_GEN_FAILURE`), which `QueryFullProcessImageNameW` also answers
/// for a process that is exiting.
pub const ERROR_GEN_FAILURE: i32 = 31;

/// Whether an error from asking Windows about a process (its raw OS error)
/// is Windows refusing to say, as it does for a process that is exiting,
/// rather than the question failing. A refusal is never an answer: it
/// can't make the process anything but what the rest of what Windows said
/// makes it.
pub fn refused(os_error: Option<i32>) -> bool {
    matches!(os_error, Some(ERROR_ACCESS_DENIED | ERROR_GEN_FAILURE))
}

impl Launched {
    /// What a running process holding this app's pid is, from when Windows
    /// says it was created and the program it runs, each `None` where
    /// Windows refused to say ([`refused`]), as it can for a process that is
    /// exiting. Only a creation time that differs, or a program that does,
    /// says it isn't the app; a refusal never does. With no creation time,
    /// nothing can tell it apart from a process given the pid since:
    /// `None`, neither the app nor another.
    pub fn identify(&self, created: Option<u64>, path: Option<&str>) -> Option<Identity> {
        let created = created?;
        if created != self.created {
            return Some(Identity::CreatedOtherwise);
        }
        Some(match path {
            None => Identity::AppExiting,
            Some(path) if path.eq_ignore_ascii_case(&self.path) => Identity::App,
            Some(_) => Identity::OtherProgram,
        })
    }

    /// Whether the process holding this app's pid, which Windows refused to
    /// open to wait for or stop, is the app, by when it was created
    /// (`created`, asked with the least access there is; `None`: no process
    /// has the pid now). Only the app's creation time makes it the app,
    /// exiting; any other is a process given the pid since, and the app is
    /// gone.
    pub fn is_by_creation(&self, created: Option<u64>) -> bool {
        created == Some(self.created)
    }

    /// The record's line for this app, newline included.
    pub fn to_line(&self) -> String {
        format!("{}\t{}\t{}\n", self.pid, self.created, self.path)
    }

    /// Every app in a record gui-launch wrote, in the order started.
    pub fn parse_all(record: &str) -> Result<Vec<Self>> {
        record
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let mut fields = line.splitn(3, '\t');
                let (Some(pid), Some(created), Some(path)) =
                    (fields.next(), fields.next(), fields.next())
                else {
                    anyhow::bail!("not a launched app's line: {line:?}");
                };
                Ok(Self {
                    pid: pid
                        .parse()
                        .with_context(|| format!("not a pid in {line:?}"))?,
                    created: created
                        .parse()
                        .with_context(|| format!("not a creation time in {line:?}"))?,
                    path: path.to_owned(),
                })
            })
            .collect()
    }
}

/// Windows processes: opened by pid, asked when they were created, what
/// program they run and whether they have exited, and terminated.
#[cfg(windows)]
pub mod process {
    use std::io;
    use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};

    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_PARAMETER, FILETIME, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
        WaitForSingleObject,
    };

    use super::{Identity, Launched, refused};

    /// A process, held open: while it is, its pid can't go to another.
    #[derive(Debug)]
    pub struct Process(OwnedHandle);

    impl Process {
        /// Opens the process `pid` to watch and stop it, or `None` if no
        /// process has that pid.
        pub fn open(pid: u32) -> io::Result<Option<Self>> {
            open(
                pid,
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
            )
        }

        /// When the process `pid` was created, or `None` if no process has
        /// that pid: asked with the least access there is
        /// (`PROCESS_QUERY_LIMITED_INFORMATION`, which Windows grants for
        /// nearly every process), for a process [`Process::open`] couldn't
        /// open.
        pub fn created_at(pid: u32) -> io::Result<Option<u64>> {
            match open(pid, PROCESS_QUERY_LIMITED_INFORMATION)? {
                Some(process) => created(process.0.as_handle()).map(Some),
                None => Ok(None),
            }
        }

        /// Whether it has exited.
        pub fn exited(&self) -> io::Result<bool> {
            // SAFETY: the handle is open (`self` owns it) and has
            // SYNCHRONIZE access; a zero timeout never blocks.
            match unsafe { WaitForSingleObject(self.0.as_raw_handle(), 0) } {
                WAIT_OBJECT_0 => Ok(true),
                WAIT_TIMEOUT => Ok(false),
                _ => Err(io::Error::last_os_error()),
            }
        }

        /// Ends it at once, with exit code 1.
        pub fn terminate(&self) -> io::Result<()> {
            // SAFETY: the handle is open (`self` owns it) and has
            // PROCESS_TERMINATE access.
            if unsafe { TerminateProcess(self.0.as_raw_handle(), 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        /// What this process, which holds `app`'s pid, is
        /// ([`Launched::identify`]): `None` when Windows won't say even when
        /// it was created. Windows tells anyone allowed to query a process
        /// when it was created, even once it's exiting, but refuses to say
        /// what program an exiting process runs (STATUS_PROCESS_IS_TERMINATING,
        /// which reaches Win32 as ERROR_ACCESS_DENIED, or ERROR_GEN_FAILURE):
        /// a refusal ([`refused`]) is taken as Windows not saying, never as
        /// an answer, and any other error is an error.
        pub fn identify(&self, app: &Launched) -> io::Result<Option<Identity>> {
            let created = said(created(self.0.as_handle()))?;
            if created != Some(app.created) {
                return Ok(app.identify(created, None));
            }
            let path = said(path(self.0.as_handle()))?;
            Ok(app.identify(created, path.as_deref()))
        }
    }

    /// What Windows answered: `None` if it refused to say ([`refused`]).
    fn said<T>(answer: io::Result<T>) -> io::Result<Option<T>> {
        match answer {
            Ok(value) => Ok(Some(value)),
            Err(e) if refused(e.raw_os_error()) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Opens the process `pid` with `access`, or `None` if no process has
    /// that pid.
    fn open(pid: u32, access: u32) -> io::Result<Option<Process>> {
        // SAFETY: OpenProcess takes no pointers; a null handle is checked
        // for before the handle is used.
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() {
            let e = io::Error::last_os_error();
            // What OpenProcess says when there is no such process.
            return match e.raw_os_error() {
                Some(code) if code == ERROR_INVALID_PARAMETER as i32 => Ok(None),
                _ => Err(e),
            };
        }
        // SAFETY: `handle` is a process handle just opened, owned by
        // nothing else, and closed only by the `OwnedHandle`.
        Ok(Some(Process(unsafe {
            OwnedHandle::from_raw_handle(handle)
        })))
    }

    /// What the record says about the process `handle` (gui-launch's own
    /// child): its pid, when it was created and its program's path.
    pub fn launched(pid: u32, handle: impl AsHandle) -> io::Result<Launched> {
        let handle = handle.as_handle();
        Ok(Launched {
            pid,
            created: created(handle)?,
            path: path(handle)?,
        })
    }

    /// When the process `handle` was created (a FILETIME as a number).
    fn created(handle: BorrowedHandle<'_>) -> io::Result<u64> {
        let mut creation = FILETIME::default();
        let (mut exit, mut kernel, mut user) = (creation, creation, creation);
        // SAFETY: the handle is open for as long as it's borrowed, with at
        // least PROCESS_QUERY_LIMITED_INFORMATION access; the four
        // pointers are to FILETIMEs that outlive the call.
        let ok = unsafe {
            GetProcessTimes(
                handle.as_raw_handle(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
    }

    /// The full path of the program the process `handle` runs.
    fn path(handle: BorrowedHandle<'_>) -> io::Result<String> {
        // Room for a long path (32,767 UTF-16 units is Windows' limit).
        let mut buffer = vec![0u16; 32_768];
        let mut len = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        // SAFETY: the handle is open for as long as it's borrowed, with at
        // least PROCESS_QUERY_LIMITED_INFORMATION access; `buffer` holds
        // `len` UTF-16 units, and the call writes at most that many and
        // sets `len` to the count written.
        let ok = unsafe {
            QueryFullProcessImageNameW(
                handle.as_raw_handle(),
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut len,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        buffer.truncate(usize::try_from(len).unwrap_or(buffer.len()));
        Ok(String::from_utf16_lossy(&buffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_reads_back_as_written() -> Result<()> {
        let app = Launched {
            pid: 4380,
            created: 134_073_438_840_000_000,
            path: r"C:\Users\runneradmin\AppData\Local\Gospel Getter\gospel_getter.exe".to_owned(),
        };
        let second = Launched {
            pid: 6120,
            ..app.clone()
        };
        let record = format!("{}{}", app.to_line(), second.to_line());
        assert_eq!(Launched::parse_all(&record)?, vec![app, second]);
        Ok(())
    }

    #[test]
    fn an_empty_record_has_no_apps() -> Result<()> {
        assert!(Launched::parse_all("")?.is_empty());
        assert!(Launched::parse_all("\n")?.is_empty());
        Ok(())
    }

    fn recorded() -> Launched {
        Launched {
            pid: 7896,
            created: 134_388_119_117_684_485,
            path: r"C:\Users\runneradmin\AppData\Local\Gospel Getter\gospel_getter.exe".to_owned(),
        }
    }

    /// Only a creation time or a program that differs says a process isn't
    /// the app; Windows refusing to name the program of the process created
    /// at the app's moment never does.
    #[test]
    fn a_process_is_the_app_by_its_creation_and_program() {
        let app = recorded();
        let created = Some(app.created);
        let shouted = app.path.to_uppercase();
        assert_eq!(app.identify(created, Some(&app.path)), Some(Identity::App));
        assert_eq!(app.identify(created, Some(&shouted)), Some(Identity::App));
        assert_eq!(app.identify(created, None), Some(Identity::AppExiting));
        assert_eq!(
            app.identify(created, Some(r"C:\Windows\System32\svchost.exe")),
            Some(Identity::OtherProgram)
        );
        for path in [Some(app.path.as_str()), None] {
            assert_eq!(
                app.identify(Some(app.created + 1), path),
                Some(Identity::CreatedOtherwise)
            );
        }
    }

    /// Run 42879721170: pid 7896, exiting, made the checks fail with
    /// "couldn't tell what pid 7896 is: Access is denied (os error 5)".
    /// Access denied, and the "device not functioning" Windows also gives
    /// for an exiting process's program, are refusals, which leave the
    /// process the app (exiting) when its creation time is the app's, and
    /// another process when it isn't. Any other error isn't a refusal, and
    /// with no creation time at all it's neither: it can't be told apart.
    #[test]
    fn a_refusal_is_never_an_answer() {
        assert!(refused(Some(ERROR_ACCESS_DENIED)));
        assert!(refused(Some(ERROR_GEN_FAILURE)));
        // ERROR_INVALID_PARAMETER (no such process), ERROR_INVALID_HANDLE.
        assert!(!refused(Some(87)));
        assert!(!refused(Some(6)));
        assert!(!refused(None));

        let app = recorded();
        assert_eq!(
            app.identify(Some(app.created), None),
            Some(Identity::AppExiting),
            "refused its program, created at the app's moment: the app, exiting"
        );
        assert_eq!(
            app.identify(Some(app.created - 1), None),
            Some(Identity::CreatedOtherwise),
            "refused its program, created at another moment: not the app"
        );
        for path in [None, Some(app.path.as_str())] {
            assert_eq!(app.identify(None, path), None, "no creation time");
        }
    }

    /// A pid Windows won't open to wait for: the app (exiting) only by the
    /// app's creation time; with another, or with no process there now,
    /// the app is gone.
    #[test]
    fn a_pid_that_wont_open_is_the_app_only_by_its_creation() {
        let app = recorded();
        assert!(app.is_by_creation(Some(app.created)));
        assert!(!app.is_by_creation(Some(app.created + 10_000_000)));
        assert!(!app.is_by_creation(None));
    }

    #[test]
    fn a_broken_line_is_refused() {
        assert!(Launched::parse_all("4380\n").is_err());
        assert!(Launched::parse_all("pid\t1\tC:\\app.exe\n").is_err());
        assert!(Launched::parse_all("4380\tlater\tC:\\app.exe\n").is_err());
    }
}
