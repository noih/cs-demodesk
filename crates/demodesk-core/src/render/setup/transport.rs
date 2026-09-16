use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use ureq::unversioned::{
    resolver::DefaultResolver,
    transport::{Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport},
};

pub(super) fn agent(cancel: Arc<AtomicBool>) -> ureq::Agent {
    ureq::Agent::with_parts(
        super::agent().config().clone(),
        DefaultConnector::default().chain(Cancellable(cancel)),
        DefaultResolver::default(),
    )
}

#[derive(Debug)]
struct Cancellable(Arc<AtomicBool>);

impl Connector<Box<dyn Transport>> for Cancellable {
    type Out = DownloadTransport;
    fn connect(
        &self,
        _: &ConnectionDetails,
        transport: Option<Box<dyn Transport>>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(transport.map(|inner| DownloadTransport {
            inner,
            cancel: self.0.clone(),
        }))
    }
}

#[derive(Debug)]
struct DownloadTransport {
    inner: Box<dyn Transport>,
    cancel: Arc<AtomicBool>,
}

impl Transport for DownloadTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let start = Instant::now();
        let budget = (*timeout.after).min(Duration::from_secs(60));
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                return Err(std::io::Error::other("Download cancelled").into());
            }
            let remaining = budget.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(ureq::Error::Timeout(timeout.reason));
            }
            // Keep polling inside the transport so HTTP parsing retains partial input.
            let poll = NextTimeout {
                after: remaining.min(Duration::from_millis(250)).into(),
                reason: timeout.reason,
            };
            match self.inner.await_input(poll) {
                Err(ureq::Error::Timeout(_)) => continue,
                result => return result,
            }
        }
    }
    fn is_open(&mut self) -> bool {
        !self.cancel.load(Ordering::Relaxed) && self.inner.is_open()
    }
    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn stalled_body_can_be_cancelled_and_slow_body_can_resume() {
        for cancel_download in [true, false] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let (release, wait) = std::sync::mpsc::channel();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = [0; 4096];
                socket.read(&mut request).unwrap();
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\na",
                    )
                    .unwrap();
                if cancel_download {
                    let _ = wait.recv_timeout(Duration::from_secs(5));
                } else {
                    std::thread::sleep(Duration::from_millis(750));
                }
                let _ = socket.write_all(b"b");
            });
            let cancel = Arc::new(AtomicBool::new(false));
            let mut reader = agent(cancel.clone())
                .get(&url)
                .call()
                .unwrap()
                .into_body()
                .into_reader();
            let mut first = [0];
            reader.read_exact(&mut first).unwrap();
            let trigger = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(400));
                cancel.store(cancel_download, Ordering::Relaxed);
            });
            let start = Instant::now();
            let mut rest = Vec::new();
            let result = reader.read_to_end(&mut rest);
            let elapsed = start.elapsed();
            let _ = release.send(());
            trigger.join().unwrap();
            server.join().unwrap();
            if cancel_download {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("Download cancelled"));
                assert!(
                    elapsed < Duration::from_secs(2),
                    "cancellation took {elapsed:?}"
                );
            } else {
                result.unwrap();
                assert_eq!(rest, b"b");
            }
        }
    }
}
