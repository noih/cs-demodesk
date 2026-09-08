//! Output-process ownership. Windows assigns the job during CreateProcess itself,
//! so even a parent crash immediately after launch cannot leave an unmanaged child.
#[cfg(windows)]
mod platform {
    use std::{ffi::OsStr, fs::{File, OpenOptions}, io::{self, Read, Seek, SeekFrom}, mem::{size_of, zeroed},
        os::windows::{ffi::OsStrExt, io::{AsRawHandle, FromRawHandle, OwnedHandle}, process::ExitStatusExt},
        process::{Command, ExitStatus, Output}, ptr::{null, null_mut}};
    use windows_sys::Win32::{Foundation::*, System::{JobObjects::*, Threading::*}};

    pub struct ProcessTree(OwnedHandle);
    pub struct Child(OwnedHandle);
    fn checked(ok: i32) -> io::Result<()> { if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) } }
    fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
        let mut text: Vec<u16> = value.encode_wide().collect();
        if text.contains(&0) { return Err(io::Error::new(io::ErrorKind::InvalidInput, "NUL in process argument")); }
        text.push(0); Ok(text)
    }
    // Microsoft CRT argv quoting, including empty arguments and trailing backslashes.
    fn argument(value: &OsStr, line: &mut Vec<u16>) -> io::Result<()> {
        line.push(b'"' as u16);
        let mut slashes = 0;
        for c in wide(value)?.into_iter().take_while(|c| *c != 0) {
            if c == b'\\' as u16 { slashes += 1; continue; }
            line.extend(std::iter::repeat_n(b'\\' as u16, slashes * if c == b'"' as u16 { 2 } else { 1 }));
            if c == b'"' as u16 { line.push(b'\\' as u16); }
            line.push(c); slashes = 0;
        }
        line.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
        line.push(b'"' as u16); Ok(())
    }
    struct Attributes(Vec<usize>);
    impl Attributes {
        fn new() -> io::Result<Self> {
            let mut bytes = 0;
            unsafe { InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes); }
            if bytes == 0 { return Err(io::Error::last_os_error()); }
            let mut data = vec![0; bytes.div_ceil(size_of::<usize>())];
            unsafe { checked(InitializeProcThreadAttributeList(data.as_mut_ptr().cast(), 2, 0, &mut bytes))?; }
            Ok(Self(data))
        }
        fn set<T>(&mut self, key: u32, values: &[T]) -> io::Result<()> {
            unsafe { checked(UpdateProcThreadAttribute(self.0.as_mut_ptr().cast(), 0, key as usize,
                values.as_ptr().cast(), std::mem::size_of_val(values), null_mut(), null())) }
        }
    }
    impl Drop for Attributes { fn drop(&mut self) { unsafe { DeleteProcThreadAttributeList(self.0.as_mut_ptr().cast()); } } }

    impl ProcessTree {
        pub fn new() -> io::Result<Self> {
            unsafe {
                let handle = CreateJobObjectW(null(), null());
                if handle.is_null() { return Err(io::Error::last_os_error()); }
                // Non-inheritable: only the app owns the kill-on-close handle.
                let tree = Self(OwnedHandle::from_raw_handle(handle));
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                checked(SetInformationJobObject(handle, JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(), size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32))?;
                Ok(tree)
            }
        }
        #[cfg(test)]
        pub fn contains_pid(&self, pid: u32) -> io::Result<bool> {
            unsafe {
                let raw = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if raw.is_null() { return Err(io::Error::last_os_error()); }
                let process = OwnedHandle::from_raw_handle(raw);
                let mut member = 0;
                checked(IsProcessInJob(process.as_raw_handle(), self.0.as_raw_handle(), &mut member))?;
                Ok(member != 0)
            }
        }
        fn members(&self) -> io::Result<Vec<OwnedHandle>> {
            let mut capacity = 32;
            loop {
                let bytes = size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() + capacity * size_of::<usize>();
                let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
                let ok = unsafe { QueryInformationJobObject(self.0.as_raw_handle(), JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr().cast(), bytes.try_into().map_err(|_| io::Error::other("process list too large"))?, null_mut()) };
                if ok == 0 {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() == Some(ERROR_MORE_DATA as i32) { capacity *= 2; continue; }
                    return Err(error);
                }
                let list = unsafe { &*buffer.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() };
                let ids = unsafe { std::slice::from_raw_parts(list.ProcessIdList.as_ptr(), list.NumberOfProcessIdsInList as usize) };
                let mut processes = vec![];
                for pid in ids {
                    unsafe {
                        let raw = OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, *pid as u32);
                        if raw.is_null() {
                            let error = io::Error::last_os_error();
                            if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) { continue; }
                            return Err(error);
                        }
                        let process = OwnedHandle::from_raw_handle(raw);
                        let mut member = 0;
                        checked(IsProcessInJob(process.as_raw_handle(), self.0.as_raw_handle(), &mut member))?;
                        if member != 0 { processes.push(process); }
                    }
                }
                return Ok(processes);
            }
        }
        /// Drain this tree before the next queued recording is allowed to start.
        pub fn finish(&self) -> io::Result<()> {
            let processes = self.members()?;
            unsafe { checked(TerminateJobObject(self.0.as_raw_handle(), 1))?; }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            // A job's active count can reach zero before the process exit is signaled.
            for process in processes {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now()).as_millis() as u32;
                match unsafe { WaitForSingleObject(process.as_raw_handle(), remaining) } {
                    WAIT_OBJECT_0 => {},
                    WAIT_TIMEOUT => return Err(io::Error::new(io::ErrorKind::TimedOut, "output process exit not signaled")),
                    _ => return Err(io::Error::last_os_error()),
                }
            }
            loop {
                let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
                unsafe { checked(QueryInformationJobObject(self.0.as_raw_handle(), JobObjectBasicAccountingInformation,
                    (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(), size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32, null_mut()))?; }
                if info.ActiveProcesses == 0 { return Ok(()); }
                if std::time::Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "output processes did not exit")); }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        /// This internal launcher supports args/env/current_dir; stdio is always NUL.
        pub fn spawn(&self, command: &mut Command) -> io::Result<Child> { self.start(command, None) }
        /// Anonymous temporary files capture output without pipe deadlocks or log archives.
        pub fn output(&self, command: &mut Command) -> io::Result<Output> {
            let mut stdout = tempfile::tempfile()?;
            let mut stderr = tempfile::tempfile()?;
            let mut child = self.start(command, Some((&stdout, &stderr)))?;
            let status = child.wait()?;
            self.finish()?;
            stdout.seek(SeekFrom::Start(0))?; stderr.seek(SeekFrom::Start(0))?;
            let mut out = Output { status, stdout: vec![], stderr: vec![] };
            stdout.read_to_end(&mut out.stdout)?; stderr.read_to_end(&mut out.stderr)?;
            Ok(out)
        }
        fn start(&self, command: &Command, capture: Option<(&File, &File)>) -> io::Result<Child> {
            let program = wide(command.get_program())?;
            let mut line = vec![];
            argument(command.get_program(), &mut line)?;
            for arg in command.get_args() { line.push(b' ' as u16); argument(arg, &mut line)?; }
            line.push(0);
            let mut env: Vec<_> = std::env::vars_os().collect();
            for (key, value) in command.get_envs() {
                env.retain(|(name, _)| !name.to_string_lossy().eq_ignore_ascii_case(&key.to_string_lossy()));
                if let Some(value) = value { env.push((key.to_os_string(), value.to_os_string())); }
            }
            env.sort_by_key(|(key, _)| key.to_string_lossy().to_uppercase());
            let mut block = vec![];
            for (key, value) in env {
                let mut entry = key; entry.push("="); entry.push(value); block.extend(wide(&entry)?);
            }
            block.push(0);
            let cwd = command.get_current_dir().map(|p| wide(p.as_os_str())).transpose()?;
            let nul = OpenOptions::new().read(true).write(true).open("NUL")?;
            let input = nul.as_raw_handle();
            let (output, error) = capture.map(|(out, err)| (out.as_raw_handle(), err.as_raw_handle())).unwrap_or((input, input));
            let mut inherited = vec![input, output, error]; inherited.sort(); inherited.dedup();
            let jobs = [self.0.as_raw_handle()];
            let mut attributes = Attributes::new()?;
            attributes.set(PROC_THREAD_ATTRIBUTE_JOB_LIST, &jobs)?;
            attributes.set(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &inherited)?;
            unsafe {
                for handle in &inherited { checked(SetHandleInformation(*handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT))?; }
                let mut startup: STARTUPINFOEXW = zeroed();
                startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
                startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
                startup.StartupInfo.hStdInput = input; startup.StartupInfo.hStdOutput = output; startup.StartupInfo.hStdError = error;
                startup.lpAttributeList = attributes.0.as_mut_ptr().cast();
                let mut info: PROCESS_INFORMATION = zeroed();
                let result = checked(CreateProcessW(program.as_ptr(), line.as_mut_ptr(), null(), null(), 1,
                    CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                    block.as_ptr().cast(), cwd.as_ref().map_or(null(), |v| v.as_ptr()), &startup.StartupInfo, &mut info));
                for handle in &inherited { SetHandleInformation(*handle, HANDLE_FLAG_INHERIT, 0); }
                result?;
                drop(OwnedHandle::from_raw_handle(info.hThread));
                Ok(Child(OwnedHandle::from_raw_handle(info.hProcess)))
            }
        }
    }
    impl Child {
        pub fn kill(&mut self) -> io::Result<()> { unsafe { checked(TerminateProcess(self.0.as_raw_handle(), 1)) } }
        pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            unsafe {
                match WaitForSingleObject(self.0.as_raw_handle(), 0) {
                    WAIT_TIMEOUT => Ok(None),
                    WAIT_OBJECT_0 => { let mut code = 0; checked(GetExitCodeProcess(self.0.as_raw_handle(), &mut code))?; Ok(Some(ExitStatus::from_raw(code))) }
                    _ => Err(io::Error::last_os_error()),
                }
            }
        }
        pub fn wait(&mut self) -> io::Result<ExitStatus> {
            if unsafe { WaitForSingleObject(self.0.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0 { return Err(io::Error::last_os_error()); }
            self.try_wait()?.ok_or_else(|| io::Error::other("process did not exit"))
        }
    }
}
#[cfg(windows)]
pub(crate) use platform::ProcessTree;

#[cfg(not(windows))]
pub(crate) struct ProcessTree;
#[cfg(not(windows))]
impl ProcessTree {
    pub fn new() -> std::io::Result<Self> { Ok(Self) }
    pub fn finish(&self) -> std::io::Result<()> { Ok(()) }
    pub fn spawn(&self, command: &mut std::process::Command) -> std::io::Result<std::process::Child> { command.spawn() }
    pub fn output(&self, command: &mut std::process::Command) -> std::io::Result<std::process::Output> { command.output() }
}

#[cfg(all(test, windows))]
mod tests {
    use super::ProcessTree;
    use std::{fs, io, os::windows::io::{FromRawHandle, OwnedHandle, AsRawHandle}, path::Path,
        process::{Command, Stdio}, thread, time::{Duration, Instant}};
    use windows_sys::Win32::System::Threading::*;
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};

    fn command(role: &str, dir: &Path) -> Command {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args(["--exact", "render::process::tests::fixture", "--nocapture"])
            .env("DEMODESK_PROCESS_TEST_ROLE", role).env("DEMODESK_PROCESS_TEST_DIR", dir)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        super::super::hide(&mut cmd);
        cmd
    }
    fn wait_for(mut condition: impl FnMut() -> bool) {
        let until = Instant::now() + Duration::from_secs(10);
        while !condition() { assert!(Instant::now() < until, "fixture timed out"); thread::sleep(Duration::from_millis(10)); }
    }
    // Invoked only in dedicated subprocesses; a deadline prevents leftovers even on test failure.
    #[test]
    fn fixture() {
        let Ok(role) = std::env::var("DEMODESK_PROCESS_TEST_ROLE") else { return };
        let dir = std::path::PathBuf::from(std::env::var_os("DEMODESK_PROCESS_TEST_DIR").unwrap());
        if role == "echo" {
            let args: Vec<_> = std::env::args().collect();
            println!("{}", serde_json::to_string(&args).unwrap());
            println!("env={}", std::env::var("DEMODESK_PROCESS_TEST_VALUE").unwrap());
            eprintln!("stderr preserved");
            return;
        }
        let mut leaf = None;
        let tree = if role == "owner" {
            let tree = ProcessTree::new().unwrap();
            tree.spawn(&mut command("branch", &dir)).unwrap();
            Some(tree)
        } else {
            if role == "branch" { leaf = Some(command("leaf", &dir).spawn().unwrap()); }
            None
        };
        fs::write(dir.join(&role), std::process::id().to_string()).unwrap();
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until && !(role == "owner" && dir.join("release").exists()) {
            thread::sleep(Duration::from_millis(20));
        }
        drop(tree);
        if let Some(mut leaf) = leaf { let _ = leaf.kill(); let _ = leaf.wait(); }
    }
    struct Owner(std::process::Child);
    impl Drop for Owner { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
    fn process_from_marker(dir: &Path, name: &str) -> OwnedHandle {
        wait_for(|| fs::read_to_string(dir.join(name)).is_ok_and(|s| s.parse::<u32>().is_ok()));
        let pid = fs::read_to_string(dir.join(name)).unwrap().parse().unwrap();
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!handle.is_null(), "{}", io::Error::last_os_error());
        unsafe { OwnedHandle::from_raw_handle(handle) }
    }
    #[test]
    fn closing_owner_or_crashing_it_kills_descendants_only() {
        for abrupt in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut unrelated = Owner(command("unrelated", dir.path()).spawn().unwrap());
            let outside = process_from_marker(dir.path(), "unrelated");
            let mut owner = Owner(command("owner", dir.path()).spawn().unwrap());
            let branch = process_from_marker(dir.path(), "branch");
            let leaf = process_from_marker(dir.path(), "leaf");
            if abrupt { owner.0.kill().unwrap(); }
            else { fs::write(dir.path().join("release"), "").unwrap(); }
            for process in [&branch, &leaf] {
                assert_eq!(unsafe { WaitForSingleObject(process.as_raw_handle(), 5000) }, WAIT_OBJECT_0);
            }
            assert_eq!(unsafe { WaitForSingleObject(outside.as_raw_handle(), 0) }, WAIT_TIMEOUT);
            unrelated.0.kill().unwrap();
        }
    }
    #[test]
    fn managed_output_preserves_arguments_environment_and_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let mut cmd = command("echo", dir.path());
        cmd.env("DEMODESK_PROCESS_TEST_VALUE", "spaces and unicode 測試");
        let args = ["", "two words", "quote\"inside", "trailing\\", "slash\\\"quote", "測試"];
        // libtest accepts all trailing values as filters; the fixture still runs via --exact.
        cmd.args(args);
        let output = ProcessTree::new().unwrap().output(&mut cmd).unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8(output.stdout).unwrap();
        let encoded = text.lines().find(|line| line.starts_with('[')).expect("argv JSON");
        let actual: Vec<String> = serde_json::from_str(encoded).unwrap();
        assert_eq!(&actual[actual.len()-args.len()..], args);
        assert!(text.contains("env=spaces and unicode 測試"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("stderr preserved"));
    }
    #[test]
    #[ignore = "requires local CS2, HLAE and FFmpeg; launches an isolated game"]
    fn real_recording_tools_use_managed_processes() {
        use std::io::{Read, Write};
        assert!(!crate::render::record::is_process_running("cs2.exe").unwrap(), "close CS2 first");
        let cs2 = std::env::var_os("DEMODESK_TEST_CS2").expect("DEMODESK_TEST_CS2");
        let hlae = std::path::PathBuf::from(std::env::var_os("DEMODESK_TEST_HLAE").expect("DEMODESK_TEST_HLAE"));
        let ffmpeg = std::env::var_os("DEMODESK_TEST_FFMPEG").expect("DEMODESK_TEST_FFMPEG");
        let dir = tempfile::tempdir().unwrap();
        let hook = crate::render::startup::prepare_hook(dir.path()).unwrap();
        let log = dir.path().join("hook.log");
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut cmd = Command::new(&hlae);
        cmd.args(["-noGui", "-noConfig", "-autoStart", "-afxDisableSteamStorage", "-customLoader", "-hookDllPath"])
            .arg(hook).arg("-hookDllPath").arg(hlae.parent().unwrap().join("x64/AfxHookSource2.dll"))
            .arg("-programPath").arg(cs2).arg("-cmdLine")
            .arg(format!("-insecure -novid -windowed -width 1920 -height 1080 -netconport {port} -afxFixNetCon -afxDisableSteamStorage +engine_no_focus_sleep 0"))
            .env("USRLOCALCSGO", dir.path()).env("DEMODESK_WINDOW_HOOK_LOG", &log);
        let tree = ProcessTree::new().unwrap();
        let _loader = tree.spawn(&mut cmd).unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut console = loop {
            if let Ok(socket) = std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)) { break socket; }
            assert!(Instant::now() < deadline, "CS2 netcon timeout: {:?}", fs::read_to_string(&log));
            thread::sleep(Duration::from_millis(100));
        };
        console.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        console.write_all(b"echo demodesk_process_tree_ready\n").unwrap();
        let mut received = String::new();
        let mut buffer = [0; 4096];
        while !received.contains("demodesk_process_tree_ready") {
            assert!(received.len() < 1024 * 1024);
            let count = console.read(&mut buffer).unwrap();
            assert_ne!(count, 0);
            received.push_str(&String::from_utf8_lossy(&buffer[..count]));
        }
        assert!(fs::read_to_string(&log).unwrap().lines().any(|s| s == "installed"));
        assert!(crate::render::record::is_process_running("cs2.exe").unwrap());
        let game_pid = crate::render::record::pid_of("cs2.exe").unwrap().unwrap();
        assert!(tree.contains_pid(game_pid).unwrap(), "CS2 did not inherit the job");
        let raw_game = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, game_pid) };
        assert!(!raw_game.is_null());
        let game_handle = unsafe { OwnedHandle::from_raw_handle(raw_game) };
        tree.finish().unwrap();
        assert_eq!(unsafe { WaitForSingleObject(game_handle.as_raw_handle(), 0) }, WAIT_OBJECT_0, "CS2 exit signal");
        assert!(!crate::render::record::is_process_running("cs2.exe").unwrap());
        let output = ProcessTree::new().unwrap().output(Command::new(ffmpeg)
            .args(["-hide_banner", "-f", "lavfi", "-i", "sine=frequency=440:duration=0.1", "-f", "null", "-"])).unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }
}
