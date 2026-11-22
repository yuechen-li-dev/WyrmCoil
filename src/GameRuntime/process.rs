use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use wyrmcoil_game::GAME_ARTIFACT;
use wyrmcoil_game::Wire::{ReadFrame, Reply, Request, WriteFrame};

pub(super) struct GameProcess {
    project: PathBuf,
    files: Option<SessionFiles>,
    child: Option<Child>,
    requests: Option<SyncSender<Request>>,
    replies: Receiver<Result<Reply, String>>,
}

struct SessionFiles {
    directory: PathBuf,
    binary: PathBuf,
    stderr: PathBuf,
}

impl SessionFiles {
    fn Stage(project: &Path) -> Result<Self, String> {
        let artifact = project.join(GAME_ARTIFACT);
        if !artifact.is_file() {
            return Err(format!(
                "Build the game first. Missing {}",
                artifact.display()
            ));
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let directory = project
            .join("target")
            .join(format!("wyrmcoil-run-{}-{nonce}", std::process::id()));
        fs::create_dir(&directory).map_err(|error| format!("stage game: {error}"))?;
        let files = Self {
            binary: directory.join(artifact.file_name().ok_or("missing game binary name")?),
            stderr: directory.join("stderr.log"),
            directory,
        };
        fs::copy(&artifact, &files.binary)
            .map_err(|error| format!("copy game session executable: {error}"))?;
        Ok(files)
    }

    fn Diagnostic(&self) -> String {
        let mut bytes = Vec::new();
        if let Ok(file) = File::open(&self.stderr) {
            let _ = file.take(2048).read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Drop for SessionFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.binary);
        let _ = fs::remove_file(&self.stderr);
        let _ = fs::remove_dir(&self.directory);
    }
}

impl GameProcess {
    pub fn Start(project: &Path) -> Result<Self, String> {
        let project = project
            .canonicalize()
            .map_err(|error| format!("open game project: {error}"))?;
        let files = SessionFiles::Stage(&project)?;
        let stderr = File::create(&files.stderr)
            .map_err(|error| format!("open game diagnostics: {error}"))?;
        let mut command = Command::new(&files.binary);
        command
            .current_dir(&project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("start game executable: {error}"))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (requests, intake) = mpsc::sync_channel(1);
        let (answers, replies) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("wyrmcoil-game-io".into())
            .spawn(move || {
                let mut input = BufWriter::new(stdin);
                let mut output = BufReader::new(stdout);
                while let Ok(request) = intake.recv() {
                    let reply = WriteFrame(&mut input, request)
                        .and_then(|()| ReadFrame(&mut output))
                        .and_then(|reply| {
                            reply.ok_or_else(|| "game closed its output pipe".into())
                        });
                    let failed = reply.is_err();
                    if answers.send(reply).is_err() || failed {
                        break;
                    }
                }
            });
        if let Err(error) = worker {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("start game pipe worker: {error}"));
        }
        Ok(Self {
            project,
            files: Some(files),
            child: Some(child),
            requests: Some(requests),
            replies,
        })
    }

    pub fn Project(&self) -> &Path {
        &self.project
    }

    pub fn Pid(&self) -> u32 {
        self.child.as_ref().expect("live process").id()
    }

    pub fn Send(&self, request: Request) -> Result<(), String> {
        self.requests
            .as_ref()
            .expect("live process")
            .try_send(request)
            .map_err(|error| self.Diagnostic(&format!("game request pipe: {error}")))
    }

    pub fn Receive(&self, wait: Duration) -> Result<Option<Reply>, String> {
        match self.replies.recv_timeout(wait) {
            Ok(Ok(reply)) => Ok(Some(reply)),
            Ok(Err(error)) => Err(self.Diagnostic(&error)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => {
                Err(self.Diagnostic("game process disconnected"))
            }
        }
    }

    fn Diagnostic(&self, error: &str) -> String {
        let stderr = self
            .files
            .as_ref()
            .expect("live process files")
            .Diagnostic();
        if stderr.is_empty() {
            error.to_string()
        } else {
            format!("{error} · {stderr}")
        }
    }
}

impl Drop for GameProcess {
    fn drop(&mut self) {
        // EOF gives a responsive guest a chance to drop its state. Reap a
        // stalled guest off the editor thread, even when its pipe is blocked.
        drop(self.requests.take());
        if let (Some(mut child), Some(files)) = (self.child.take(), self.files.take()) {
            thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_millis(150);
                loop {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        break;
                    }
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                drop(files);
            });
        }
    }
}
