use crate::render::SetupTool;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::Duration;

#[derive(Default)]
struct State {
    users: [usize; 3],
    waiting: [usize; 3],
    replacing: [bool; 3],
}

#[derive(Default)]
pub(super) struct ToolAccess {
    state: Mutex<State>,
    changed: Condvar,
}

fn index(tool: SetupTool) -> usize {
    match tool {
        SetupTool::Hlae => 0,
        SetupTool::Ffmpeg => 1,
        SetupTool::Vrf => 2,
    }
}

pub(super) struct Lease {
    access: Arc<ToolAccess>,
    tools: Vec<usize>,
    exclusive: bool,
}

impl ToolAccess {
    pub(super) fn use_tools(self: &Arc<Self>, tools: &[SetupTool]) -> Lease {
        let tools: Vec<_> = tools.iter().copied().map(index).collect();
        let mut state = self.state.lock().unwrap();
        while tools
            .iter()
            .any(|&i| state.waiting[i] > 0 || state.replacing[i])
        {
            state = self.changed.wait(state).unwrap();
        }
        for &i in &tools {
            state.users[i] += 1;
        }
        Lease {
            access: self.clone(),
            tools,
            exclusive: false,
        }
    }

    pub(super) fn replace(
        self: &Arc<Self>,
        tool: SetupTool,
        cancel: &AtomicBool,
    ) -> anyhow::Result<Lease> {
        let i = index(tool);
        let mut state = self.state.lock().unwrap();
        state.waiting[i] += 1;
        loop {
            if cancel.load(Ordering::Relaxed) {
                state.waiting[i] -= 1;
                self.changed.notify_all();
                anyhow::bail!("Download cancelled");
            }
            if state.users[i] == 0 && !state.replacing[i] {
                break;
            }
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(250))
                .unwrap()
                .0;
        }
        state.waiting[i] -= 1;
        state.replacing[i] = true;
        Ok(Lease {
            access: self.clone(),
            tools: vec![i],
            exclusive: true,
        })
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.access.state.lock().unwrap();
        for &i in &self.tools {
            if self.exclusive {
                state.replacing[i] = false;
            } else {
                state.users[i] -= 1;
            }
        }
        self.access.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_drains_existing_users_blocks_new_work_and_releases_on_cancel() {
        let access = Arc::new(ToolAccess::default());
        let active = access.use_tools(&[SetupTool::Ffmpeg]);
        let cancel = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let (send, receive) = std::sync::mpsc::channel();
            let (release, wait_release) = std::sync::mpsc::channel();
            let worker_access = access.clone();
            let worker_cancel = &cancel;
            scope.spawn(move || {
                let replacement = worker_access
                    .replace(SetupTool::Ffmpeg, worker_cancel)
                    .unwrap();
                send.send(()).unwrap();
                wait_release.recv_timeout(Duration::from_secs(3)).unwrap();
                drop(replacement);
            });
            while access.state.lock().unwrap().waiting[1] == 0 {
                std::thread::yield_now();
            }
            assert!(receive.try_recv().is_err());
            let unrelated = access.use_tools(&[SetupTool::Vrf]);
            drop(unrelated);
            drop(active);
            receive.recv_timeout(Duration::from_secs(2)).unwrap();
            let (entered, started) = std::sync::mpsc::channel();
            let next_access = access.clone();
            scope.spawn(move || {
                let _next = next_access.use_tools(&[SetupTool::Ffmpeg]);
                entered.send(()).unwrap();
            });
            assert!(started.recv_timeout(Duration::from_millis(100)).is_err());
            release.send(()).unwrap();
            started.recv_timeout(Duration::from_secs(2)).unwrap();
            let next = access.use_tools(&[SetupTool::Ffmpeg]);
            assert!(!access.state.lock().unwrap().replacing[1]);
            cancel.store(true, Ordering::Relaxed);
            assert!(access.replace(SetupTool::Ffmpeg, &cancel).is_err());
            drop(next);
            assert_eq!(access.state.lock().unwrap().waiting[1], 0);
        });
    }
}
