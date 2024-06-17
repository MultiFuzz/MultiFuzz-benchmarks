use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    path::PathBuf,
    time::Duration,
};

use crate::{IpcWrapper, Request, Response, RunCommand, RunOutput};
use anyhow::Context;

pub fn connect_to_tcp_agent(addr: &str) -> anyhow::Result<Box<dyn Agent>> {
    const MAX_RETRIES: usize = 3;
    const RETRY_DELAY: Duration = Duration::from_secs(5);
    let agent = retry(|| Ok(TcpAgent::connect(addr)), MAX_RETRIES, RETRY_DELAY)?;
    Ok(Box::new(agent))
}

#[cfg(unix)]
pub fn connect_to_uds_agent(path: &str) -> anyhow::Result<Box<dyn Agent>> {
    const MAX_RETRIES: usize = 3;
    const RETRY_DELAY: Duration = Duration::from_secs(5);
    let agent = retry(|| Ok(unix::UnixAgent::connect(path.as_ref())), MAX_RETRIES, RETRY_DELAY)?;
    Ok(Box::new(agent))
}

pub fn retry<T>(
    f: impl Fn() -> anyhow::Result<anyhow::Result<T>>,
    max_retries: usize,
    retry_delay: Duration,
) -> anyhow::Result<T> {
    let mut retries = 0;
    loop {
        match f()? {
            Ok(x) => return Ok(x),
            Err(e) if retries < max_retries => {
                tracing::warn!("Error connecting to agent: {e:#}");
                retries += 1;
                std::thread::sleep(retry_delay);
            }
            Err(e) => return Err(e),
        }
    }
}

pub trait Agent {
    fn send_request(
        &mut self,
        request: Request,
        read_timeout: Option<Duration>,
    ) -> anyhow::Result<Response>;

    fn send_with_timeout(
        &mut self,
        request: Request,
        read_timeout: Option<Duration>,
    ) -> anyhow::Result<serde_json::Value> {
        match self.send_request(request, read_timeout)? {
            Response::Value(v) => Ok(v),
            Response::Error { error } => anyhow::bail!("{}", error),
        }
    }

    fn send(&mut self, request: Request) -> anyhow::Result<serde_json::Value> {
        self.send_with_timeout(request, Some(std::time::Duration::from_secs(10)))
    }

    /// Get any stats collected by the agent.
    fn get_stats(&mut self) -> anyhow::Result<String> {
        let value = self.send(Request::GetStats).context("error getting stats")?;
        Ok(serde_json::from_value(value).context("invalid stats response")?)
    }

    /// Run `task` in the background on the guest, returning the `pid` of the background process
    fn spawn_task(&mut self, task: RunCommand) -> anyhow::Result<u32> {
        let value = self.send(Request::SpawnProcess(task)).context("error spawning process")?;
        Ok(serde_json::from_value(value)
            .context("failed to read pid, invalid response from agent")?)
    }

    /// Run `task` in the guest and wait for it to complete, returning the result.
    fn run_task(&mut self, task: RunCommand) -> anyhow::Result<RunOutput> {
        let timeout = task.timeout;
        let value = self
            .send_with_timeout(Request::RunProcess(task), timeout)
            .context("error running process")?;
        Ok(serde_json::from_value(value)
            .context("failed process output, invalid response from agent")?)
    }

    /// Waits for the process associated `pid` to exit, returning its status.
    fn wait_pid(&mut self, pid: u32) -> anyhow::Result<Option<i64>> {
        let value = self
            .send_with_timeout(Request::WaitPid(pid), None)
            .context("error waiting for process exit")?;
        Ok(value.as_i64())
    }

    /// Get the status of the process associated `pid`.
    fn get_status(&mut self, pid: u32) -> anyhow::Result<Option<i64>> {
        let value = self.send(Request::GetStatus(pid)).context("error checking process status")?;
        Ok(value.as_i64())
    }

    /// Read the file at `path` from the guest.
    fn read_file(&mut self, path: PathBuf) -> anyhow::Result<Vec<u8>> {
        let value = self
            .send(Request::ReadFile { path: path.clone(), offset: 0, len: None })
            .with_context(|| format!("error reading file: {}", path.display()))?;
        serde_json::from_value(value).context("failed to read file, invalid response from agent")
    }

    /// Get metadata about the file at `path`.
    fn stat(&mut self, path: PathBuf) -> anyhow::Result<crate::DirEntry> {
        let value = self
            .send(Request::StatFile(path.clone()))
            .with_context(|| format!("error reading file metadata: {}", path.display()))?;
        serde_json::from_value(value)
            .context("failed to read file metadata, invalid response from agent")
    }

    /// Read the directory at `path` from the guest.
    fn read_dir(&mut self, path: PathBuf) -> anyhow::Result<Vec<crate::DirEntry>> {
        let value = self
            .send(Request::ReadDir(path.clone()))
            .with_context(|| format!("error reading directory: {}", path.display()))?;
        serde_json::from_value(value)
            .context("failed to read directory, invalid response from agent")
    }

    /// Send `signal` to the process `pid` running on the guest.
    fn kill_process(&mut self, pid: u32, signal: i32) -> anyhow::Result<()> {
        self.send(Request::KillProcess { pid, signal })
            .with_context(|| format!("error sending SIGNAL {signal} to process"))?;
        Ok(())
    }

    /// Shutdown the VM by sending a reboot command.
    fn shutdown_vm(&mut self) -> anyhow::Result<()> {
        self.send(Request::Reboot).context("error shutting down vm")?;
        Ok(())
    }

    /// Tell the agent to exit.
    fn exit(&mut self) -> anyhow::Result<()> {
        self.send(Request::RestartAgent).context("error restarting agent")?;
        Ok(())
    }
}

pub trait SetReadTimeout<R> {
    fn set_read_timeout(reader: &mut R, duration: Option<Duration>) -> anyhow::Result<()>;
}

pub struct RpcAgent<R: BufRead, W: Write, S: SetReadTimeout<R>> {
    pub reader: R,
    pub writer: W,
    buf: Vec<u8>,
    next_request: u64,
    set_read_timeout: std::marker::PhantomData<S>,
}

impl<R, W, S> RpcAgent<R, W, S>
where
    R: BufRead,
    W: Write,
    S: SetReadTimeout<R>,
{
    fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            buf: vec![],
            next_request: 1,
            set_read_timeout: std::marker::PhantomData,
        }
    }

    fn read_response(
        &mut self,
        read_timeout: Option<Duration>,
    ) -> anyhow::Result<IpcWrapper<Response>> {
        self.buf.clear();
        S::set_read_timeout(&mut self.reader, read_timeout)?;
        self.reader.read_until(b'\n', &mut self.buf).context("failed to read response")?;
        Ok(serde_json::from_slice(&self.buf).context("invalid response from agent")?)
    }

    fn write_request(&mut self, request_id: u64, request: Request) -> anyhow::Result<()> {
        self.buf.clear();
        serde_json::to_writer(&mut self.buf, &IpcWrapper { id: request_id, body: request })?;
        self.buf.push(b'\n');
        self.writer.write_all(&mut self.buf).context("failed to send request")
    }
}

impl<R, W, S> Agent for RpcAgent<R, W, S>
where
    W: std::io::Write,
    R: BufRead,
    S: SetReadTimeout<R>,
{
    fn send_request(
        &mut self,
        request: Request,
        read_timeout: Option<Duration>,
    ) -> anyhow::Result<Response> {
        let request_id = self.next_request;
        self.next_request += 1;

        self.write_request(request_id, request)?;
        loop {
            let IpcWrapper { id, body: response } = self.read_response(read_timeout)?;
            match id.cmp(&request_id) {
                std::cmp::Ordering::Less => {
                    tracing::warn!(
                        "agent returned stale request (wanted: {request_id}, got: {id}): {}",
                        self.buf.escape_ascii()
                    );
                }
                std::cmp::Ordering::Equal => return Ok(response),
                std::cmp::Ordering::Greater => {
                    anyhow::bail!("unexpected response: wanted: id={request_id}, got: id={id}")
                }
            }
        }
    }
}

pub struct SetTcpStreamTimeout;

impl SetReadTimeout<BufReader<TcpStream>> for SetTcpStreamTimeout {
    fn set_read_timeout(
        reader: &mut BufReader<TcpStream>,
        duration: Option<Duration>,
    ) -> anyhow::Result<()> {
        reader.get_mut().set_read_timeout(duration).context("error setting timeout")
    }
}

pub type TcpAgent = RpcAgent<BufReader<TcpStream>, TcpStream, SetTcpStreamTimeout>;

impl TcpAgent {
    pub fn connect(addr: &str) -> anyhow::Result<TcpAgent> {
        let socket = TcpStream::connect(addr)?;
        let writer = socket.try_clone()?;
        Ok(RpcAgent::new(BufReader::new(socket), writer))
    }
}

#[cfg(unix)]
pub mod unix {
    //! Unix agent connection utilizing Unix domain sockets

    use std::{io::BufReader, os::unix::net::UnixStream, path::Path, time::Duration};

    use crate::client::{RpcAgent, SetReadTimeout};
    use anyhow::Context;

    pub struct SetUnixStreamTimeout;

    impl SetReadTimeout<BufReader<UnixStream>> for SetUnixStreamTimeout {
        fn set_read_timeout(
            reader: &mut BufReader<UnixStream>,
            duration: Option<Duration>,
        ) -> anyhow::Result<()> {
            reader.get_mut().set_read_timeout(duration).context("error setting timeout")
        }
    }

    pub type UnixAgent = RpcAgent<BufReader<UnixStream>, UnixStream, SetUnixStreamTimeout>;

    impl UnixAgent {
        pub fn connect(path: &Path) -> anyhow::Result<Self> {
            let stream = UnixStream::connect(path)
                .with_context(|| format!("failed to connect to agent at: {}", path.display()))?;

            stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
            stream.set_write_timeout(Some(std::time::Duration::from_secs(10)))?;

            let writer = stream.try_clone().context("failed to clone stream")?;
            Ok(RpcAgent::new(BufReader::new(stream), writer))
        }
    }
}
