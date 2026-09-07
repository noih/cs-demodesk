//! Runs one CS2 recording session: launch CS2 through HLAE with a netcon port,
//! push the command schedule into HLAE's `mirv_cmd` system over that netcon,
//! start the demo, follow the `[demodesk]` echo markers, wait for the game to quit.
//!
//! No server plugin and no game-file patching: HLAE is injected into the client
//! anyway, its command system is tick-driven, and the netcon (`-netconport`, made
//! to work under HLAE by `-afxFixNetCon`) is the game's own remote console.

use super::actions::{mirv_cmd_xml, sequence_folder_name, Scheduled, MARK};
use super::setup::register_ffmpeg_with_hlae;
use super::hide;
use anyhow::{anyhow, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const NETCON_PORT: u16 = 4577;

pub struct RecordSession<'a> {
    pub demo_path: PathBuf,
    pub cs2_dir: PathBuf,
    pub cs2_exe: PathBuf,
    pub hlae_exe: PathBuf,
    pub hlae_dll: PathBuf,
    pub ffmpeg_exe: PathBuf,
    pub output_dir: PathBuf,
    /// USRLOCALCSGO: where this launch keeps its cfg / video settings (shared by
    /// every job so the game does not start from a blank profile each time)
    pub cfg_dir: PathBuf,
    pub width: u32,
    pub height: u32,
    pub schedule: Vec<Scheduled>,
    /// Seconds to wait for the game before giving up
    pub timeout_seconds: u64,
    pub extra_launch_options: Vec<String>,
    /// Set to true to abort (kills the game, cleans up)
    pub cancel: Arc<AtomicBool>,
    pub log: &'a mut dyn FnMut(String),
    pub stage: &'a mut dyn FnMut(&str),
}

fn tasklist(image: &str, verbose: bool) -> String {
    let mut cmd = Command::new("tasklist");
    cmd.args(["/fi", &format!("imagename eq {image}"), "/nh"]);
    if verbose {
        cmd.arg("/v");
    }
    hide(&mut cmd).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
}

pub fn is_process_running(image: &str) -> bool {
    tasklist(image, false).to_lowercase().contains(&image.to_lowercase())
}

/// PID of the first process with this image name (tasklist /nh: "cs2.exe  12345 Console …").
fn pid_of(image: &str) -> Option<u32> {
    tasklist(image, false).lines().find(|l| l.to_lowercase().contains(&image.to_lowercase())).and_then(|l| l.split_whitespace().nth(1)).and_then(|p| p.parse().ok())
}

pub fn kill_process(image: &str) {
    let mut cmd = Command::new("taskkill");
    cmd.args(["/f", "/im", image]).stdout(Stdio::null()).stderr(Stdio::null());
    let _ = hide(&mut cmd).status();
}

fn newest_crash_dump(cs2_exe: &Path, since: std::time::SystemTime) -> Option<PathBuf> {
    let dir = cs2_exe.parent()?;
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.extension().map(|e| e == "mdmp").unwrap_or(false) && p.metadata().and_then(|m| m.modified()).map(|m| m > since).unwrap_or(false)
    })
}

fn console_log_path(cs2_dir: &Path) -> PathBuf {
    cs2_dir.join("game").join("csgo").join("console.log")
}

/// Tail of the game's own console log (-condebug), filtered to lines that
/// explain why a demo did not play.
pub fn summarize_console_log(cs2_dir: &Path, log: &mut dyn FnMut(String)) {
    let path = console_log_path(cs2_dir);
    let Ok(text) = std::fs::read_to_string(&path) else {
        log(format!("game console log not found ({})", path.display()));
        return;
    };
    let keys = ["demo", "Demo", "playdemo", "rror", "ailed", "ouldn't", "Unable", "Host_", "Connect", "Disconnect", "mirv", "netcon", MARK];
    let lines: Vec<&str> = text.lines().filter(|l| keys.iter().any(|k| l.contains(k))).collect();
    log(format!("game console log: {} ({} lines, {} relevant)", path.display(), text.lines().count(), lines.len()));
    for l in lines.iter().rev().take(25).rev() {
        log(format!("  {}", l.trim()));
    }
}

pub fn hlae_args(s: &RecordSession) -> Vec<String> {
    // TrueView (cl_demo_predict) is forced off at start; the schedule switches it
    // on later when the user asked for it.
    let mut game_args = vec![
        "-insecure".to_string(),
        "-novid".to_string(),
        "-netconport".to_string(),
        NETCON_PORT.to_string(),
        "-afxFixNetCon".to_string(),
        "-afxDisableSteamStorage".to_string(),
        "+cl_demo_predict".to_string(),
        "0".to_string(),
        "+engine_no_focus_sleep".to_string(),
        "0".to_string(),
        // the demo playback UI is on by default and must be off *before* playback starts
        "+demo_ui_mode".to_string(),
        "0".to_string(),
        // draw the spectated player's own crosshair (also set at the setup tick; here in
        // case the engine only reads it while loading)
        "+cl_show_observer_crosshair".to_string(),
        "2".to_string(),
        "-width".to_string(),
        s.width.to_string(),
        "-height".to_string(),
        s.height.to_string(),
        "-sw".to_string(),
        // write csgo/console.log so a failure can be diagnosed
        "-condebug".to_string(),
    ];
    game_args.extend(s.extra_launch_options.iter().cloned());
    vec![
        "-noGui".into(),
        "-autoStart".into(),
        "-noConfig".into(),
        "-afxDisableSteamStorage".into(),
        "-customLoader".into(),
        "-hookDllPath".into(),
        s.hlae_dll.to_string_lossy().to_string(),
        "-programPath".into(),
        s.cs2_exe.to_string_lossy().to_string(),
        "-cmdLine".into(),
        game_args.join(" "),
    ]
}

/// Line-oriented client for the game's netcon (`-netconport`).
struct Netcon {
    stream: TcpStream,
    lines: mpsc::Receiver<String>,
}

impl Netcon {
    fn connect(port: u16) -> Result<Self> {
        let stream = TcpStream::connect_timeout(&format!("127.0.0.1:{port}").parse().unwrap(), Duration::from_secs(2))?;
        stream.set_nodelay(true)?;
        let reader = BufReader::new(stream.try_clone()?);
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new().name("netcon-reader".into()).spawn(move || {
            for line in reader.split(b'\n').flatten() {
                let text = String::from_utf8_lossy(&line).trim_end_matches('\r').to_string();
                if tx.send(text).is_err() {
                    break;
                }
            }
        })?;
        Ok(Self { stream, lines: rx })
    }
    fn send(&mut self, cmd: &str) -> Result<()> {
        self.stream.write_all(cmd.as_bytes())?;
        self.stream.write_all(b"\n")?;
        self.stream.flush()?;
        Ok(())
    }
    /// Drain everything received so far.
    fn drain(&self) -> Vec<String> {
        let mut out = vec![];
        while let Ok(l) = self.lines.try_recv() {
            out.push(l);
        }
        out
    }
    /// Send `echo <token>` and wait for it to come back — proves the console is alive.
    fn ping(&mut self, token: &str, timeout: Duration) -> Result<bool> {
        self.send(&format!("echo {token}"))?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.lines.recv_timeout(Duration::from_millis(200)) {
                Ok(l) if l.contains(token) => return Ok(true),
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(anyhow!("netcon closed")),
            }
        }
        Ok(false)
    }
}

/// Keeps the game window hidden from the moment it appears: polls every 200 ms
/// from process start (splash / loading screens included) and hides it again
/// whenever the engine shows it. Stops when dropped.
struct WindowHider {
    stop: Arc<AtomicBool>,
    hidden: Arc<AtomicU32>,
}

impl WindowHider {
    fn start(pid: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let hidden = Arc::new(AtomicU32::new(0));
        let (s, h) = (stop.clone(), hidden.clone());
        let _ = std::thread::Builder::new().name("window-hider".into()).spawn(move || {
            while !s.load(Ordering::Relaxed) {
                if let Ok(true) = super::window::hide_game_window(pid) {
                    h.fetch_add(1, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        });
        Self { stop, hidden }
    }
    fn hidden(&self) -> u32 {
        self.hidden.load(Ordering::Relaxed)
    }
}

impl Drop for WindowHider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Progress derived from the `[demodesk] seq i of n …` markers.
fn parse_marker(line: &str) -> Option<(usize, usize, &str)> {
    let rest = line.split(MARK).nth(1)?.trim();
    let mut it = rest.split_whitespace();
    if it.next()? != "seq" {
        return None;
    }
    let i: usize = it.next()?.parse().ok()?;
    if it.next()? != "of" {
        return None;
    }
    let n: usize = it.next()?.parse().ok()?;
    let what = it.next()?;
    Some((i, n, what))
}

/// Marker-driven state of a running schedule.
#[derive(Default)]
struct Progress {
    seen_marker: bool,
    done: bool,
}

impl RecordSession<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Start CS2 through HLAE and wait until the injected process is up; the
    /// returned hider keeps its window off screen from here on.
    fn launch(&mut self) -> Result<Option<WindowHider>> {
        // Keep the player's real config untouched: the game reads/writes cfg under USRLOCALCSGO.
        std::fs::create_dir_all(&self.cfg_dir)?;
        let args = hlae_args(self);
        (self.log)(format!("launching HLAE: {} {}", self.hlae_exe.display(), args.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }).collect::<Vec<_>>().join(" ")));
        let mut cmd = Command::new(&self.hlae_exe);
        cmd.args(&args).env("USRLOCALCSGO", &self.cfg_dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let mut hlae = hide(&mut cmd).spawn()?;

        // HLAE is only a loader; it returns quickly once cs2.exe has been injected.
        let deadline = Instant::now() + Duration::from_secs(60);
        while !is_process_running("cs2.exe") {
            if Instant::now() > deadline {
                let _ = hlae.kill();
                return Err(anyhow!("cs2.exe did not start within 60 s"));
            }
            if self.cancelled() {
                let _ = hlae.kill();
                return Err(anyhow!("cancelled"));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        // The window shows up during engine init (engine_no_focus_sleep 0 keeps it
        // rendering while hidden, verified in practice), so start hiding right away.
        let hider = pid_of("cs2.exe").map(WindowHider::start);
        if hider.is_none() {
            (self.log)("cs2.exe pid not found — window stays on screen".into());
        }
        std::thread::sleep(Duration::from_secs(3));
        if tasklist("cs2.exe", true).contains("Error - AfxHookSource") {
            kill_process("cs2.exe");
            return Err(anyhow!("HLAE injection failed (AfxHookSource error window)"));
        }
        if let Ok(Some(status)) = hlae.try_wait() {
            if !status.success() {
                (self.log)(format!("HLAE exited with {status}"));
            }
        }
        Ok(hider)
    }

    /// Connect to the game's netcon once the engine answers.
    fn connect_netcon(&mut self, hider: Option<&WindowHider>) -> Result<Netcon> {
        (self.log)(format!("game running, connecting to netcon on port {NETCON_PORT}…"));
        let deadline = Instant::now() + Duration::from_secs(120);
        let con = loop {
            if !is_process_running("cs2.exe") {
                return Err(anyhow!("cs2.exe exited before the console came up"));
            }
            if self.cancelled() {
                kill_process("cs2.exe");
                return Err(anyhow!("cancelled"));
            }
            if Instant::now() > deadline {
                kill_process("cs2.exe");
                return Err(anyhow!("netcon did not come up within 120 s (-netconport {NETCON_PORT} / -afxFixNetCon)"));
            }
            if let Ok(mut c) = Netcon::connect(NETCON_PORT) {
                if c.ping("demodesk_hello", Duration::from_secs(5))? {
                    break c;
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        };
        (self.log)("netcon connected".into());
        match hider.map(WindowHider::hidden) {
            Some(0) => (self.log)("game window not seen yet — it stays on screen".into()),
            Some(n) => (self.log)(format!("game window hidden ({n}× so far)")),
            None => {}
        }
        Ok(con)
    }

    /// Load the schedule into HLAE and start the demo.
    fn start_demo(&mut self, con: &mut Netcon, schedule_file: &Path) -> Result<()> {
        let xml = schedule_file.to_string_lossy().replace('\\', "/");
        con.send("mirv_cmd clear")?;
        con.send(&format!("mirv_cmd load \"{xml}\""))?;
        if !con.ping("demodesk_loaded", Duration::from_secs(5))? {
            (self.log)("warning: console did not acknowledge the schedule load".into());
        }
        let noise = con.drain();
        if let Some(err) = noise.iter().find(|l| l.to_lowercase().contains("unknown command") && l.contains("mirv_cmd")) {
            kill_process("cs2.exe");
            return Err(anyhow!("HLAE is not active in this game process ({err})"));
        }
        (self.log)(format!("schedule loaded ({} commands)", self.schedule.len()));
        con.send("demo_ui_mode 0")?;
        self.playdemo(con)?;
        (self.log)("playdemo sent, waiting for the demo to start…".into());
        Ok(())
    }

    fn playdemo(&self, con: &mut Netcon) -> Result<()> {
        // Native backslash path in quotes: forward slashes were not accepted by playdemo.
        con.send(&format!("playdemo \"{}\"", self.demo_path.display()))
    }

    /// Follow the `[demodesk]` markers until the game quits; `Ok(true)` when the
    /// schedule ran to its end.
    fn follow_markers(&mut self, con: &mut Netcon) -> Result<bool> {
        let timeout_at = Instant::now() + Duration::from_secs(self.timeout_seconds);
        let demo_deadline = Instant::now() + Duration::from_secs(120);
        let mut progress = Progress::default();
        let mut replayed = false;
        while is_process_running("cs2.exe") {
            if self.cancelled() {
                let _ = con.send("quit");
                std::thread::sleep(Duration::from_secs(2));
                kill_process("cs2.exe");
                return Err(anyhow!("cancelled"));
            }
            for line in con.drain() {
                self.handle_console_line(&line, &mut progress);
            }
            if !progress.seen_marker && Instant::now() > demo_deadline {
                if replayed {
                    kill_process("cs2.exe");
                    return Err(anyhow!("the demo did not reach the first clip within 4 minutes — see the log"));
                }
                replayed = true;
                (self.log)("no marker yet — sending playdemo again".into());
                self.playdemo(con)?;
            }
            if Instant::now() > timeout_at {
                kill_process("cs2.exe");
                return Err(anyhow!("recording timed out after {} s", self.timeout_seconds));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Ok(progress.done)
    }

    fn handle_console_line(&mut self, line: &str, progress: &mut Progress) {
        if let Some((i, n, what)) = parse_marker(line) {
            progress.seen_marker = true;
            match what {
                "seek" => (self.stage)(&format!("recording {i}/{n}: seeking")),
                "setup" => (self.stage)(&format!("recording {i}/{n}: setup")),
                "start" => {
                    (self.stage)(&format!("recording {i}/{n}"));
                    (self.log)(format!("clip {i}/{n}: recording"));
                }
                "end" => (self.log)(format!("clip {i}/{n}: done")),
                _ => {}
            }
        } else if line.contains(MARK) && line.contains("done") {
            progress.done = true;
            (self.log)("all clips recorded, waiting for the game to quit".into());
        } else if line.contains("single_player_pause") {
            // the engine runs this Source-1 leftover whenever the window loses focus (we hide it); noise
        } else if line.contains("Unknown command") || (line.contains("mirv_streams") && line.to_lowercase().contains("error")) {
            (self.log)(format!("console: {}", line.trim()));
        }
    }

    fn run(&mut self, schedule_file: &Path, started_at: std::time::SystemTime) -> Result<()> {
        let hider = self.launch()?;
        let mut con = self.connect_netcon(hider.as_ref())?;
        self.start_demo(&mut con, schedule_file)?;
        if !self.follow_markers(&mut con)? {
            (self.log)("game exited before the schedule finished".into());
        }
        if let Some(dump) = newest_crash_dump(&self.cs2_exe, started_at) {
            (self.log)(format!("warning: CS2 wrote a crash dump ({}) — HLAE may be out of date", dump.file_name().unwrap().to_string_lossy()));
        }
        Ok(())
    }
}

pub fn run_recording_session(s: &mut RecordSession) -> Result<()> {
    if is_process_running("cs2.exe") {
        return Err(anyhow!("cs2.exe is already running — close the game first"));
    }
    std::fs::create_dir_all(&s.output_dir)?;
    let schedule_file = s.output_dir.join("commands.xml");
    std::fs::write(&schedule_file, mirv_cmd_xml(&s.schedule))?;
    register_ffmpeg_with_hlae(&s.hlae_exe, &s.ffmpeg_exe)?;
    let started_at = std::time::SystemTime::now();
    let _ = std::fs::remove_file(console_log_path(&s.cs2_dir));

    let result = s.run(&schedule_file, started_at);
    if result.is_err() {
        summarize_console_log(&s.cs2_dir, s.log);
    }
    result
}

#[derive(Debug, Clone)]
pub struct ClipOutput {
    pub index: usize,
    pub video: Option<PathBuf>,
    pub audio: Option<PathBuf>,
}

/// Find `video.<ext>` and the newest take's `audio.wav` for every sequence folder.
pub fn collect_clip_outputs(output_dir: &Path, count: usize, container: &str) -> Vec<ClipOutput> {
    (0..count)
        .map(|i| {
            let dir = output_dir.join(sequence_folder_name(i));
            let video = dir.join(format!("video.{container}"));
            let mut takes: Vec<PathBuf> = std::fs::read_dir(&dir)
                .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && p.file_name().map(|f| f.to_string_lossy().to_lowercase().starts_with("take")).unwrap_or(false)).collect())
                .unwrap_or_default();
            takes.sort();
            let audio = takes.last().map(|t| t.join("audio.wav")).filter(|p| p.is_file());
            ClipOutput { index: i, video: video.is_file().then_some(video), audio }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_parse() {
        assert_eq!(parse_marker("[demodesk] seq 2 of 5 start"), Some((2, 5, "start")));
        assert_eq!(parse_marker("some prefix [demodesk] seq 1 of 1 end "), Some((1, 1, "end")));
        assert_eq!(parse_marker("[demodesk] done"), None);
        assert_eq!(parse_marker("unrelated"), None);
    }
}
