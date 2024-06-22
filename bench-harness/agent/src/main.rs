use std::{
    io::{BufRead, BufReader, Write},
    process,
    sync::{Arc, Mutex},
};

use agent::{log_collector, AgentState, Exit};
use agent_interface::{IpcWrapper, Request};
use anyhow::Context;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

fn main() {
    eprintln!("[agent] v{VERSION}");

    let error = std::panic::catch_unwind(|| {
        if let Err(e) = run() {
            eprintln!("Error: {:?}", e);
            process::exit(1);
        }
    });

    if let Err(_) = error {
        eprintln!("[agent] encountered fatal error!");
    }
}

fn run() -> anyhow::Result<()> {
    let statsd = match std::env::var_os("STATSD") {
        Some(_) => log_collector::spawn(),
        None => Arc::new(Mutex::new(log_collector::StatsdData::new(0))),
    };
    let mut state = AgentState::new(statsd);

    let mut args = std::env::args();
    let _ = args.next();

    match (args.next().as_deref(), args.next().as_deref()) {
        (Some("-u"), Some(path)) => listen_unix_socket(&mut state, path)?,
        (Some("-t"), Some(addr)) => listen_tcp(&mut state, addr)?,
        (None, None) => listen_vsock(&mut state)?,
        (_, _) => eprintln!("[agent] invalid arguments"),
    }

    state.kill_all()?;

    Ok(())
}

fn listen_tcp(state: &mut AgentState, addr: &str) -> anyhow::Result<()> {
    let listener = std::net::TcpListener::bind(addr)
        .with_context(|| format!("Failed to bind to: {}", addr))?;

    for stream in listener.incoming() {
        let stream = stream.context("connect error")?;
        eprintln!("[agent] client connected: {:?}", stream);

        let writer = stream.try_clone().context("error cloning stream")?;
        let reader = BufReader::new(stream);

        match handle_connection_rpc(state, reader, writer) {
            Err(e) => eprintln!("[agent] client error: {}", e),
            Ok(false) => eprintln!("[agent] client disconnected"),
            Ok(true) => {
                eprintln!("[agent] exiting");
                break;
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
fn listen_unix_socket(state: &mut AgentState, path: &str) -> anyhow::Result<()> {
    let listener = std::os::unix::net::UnixListener::bind(path)
        .with_context(|| format!("Failed to bind to: {path}"))?;

    for stream in listener.incoming() {
        let stream = stream.context("connect error")?;
        eprintln!("[agent] client connected: {:?}", stream);

        let writer = stream.try_clone().context("error cloning stream")?;
        let reader = BufReader::new(stream);

        match handle_connection_rpc(state, reader, writer) {
            Err(e) => eprintln!("[agent] client error: {}", e),
            Ok(false) => eprintln!("[agent] client disconnected"),
            Ok(true) => {
                eprintln!("[agent] exiting");
                break;
            }
        }
    }

    Ok(())
}

#[cfg(not(unix))]
fn listen_unix_socket(_state: &mut AgentState) -> anyhow::Result<()> {
    anyhow::bail!("unix connection not supported on current platform");
}

#[cfg(not(unix))]
fn listen_vsock(_state: &mut AgentState) -> anyhow::Result<()> {
    anyhow::bail!("vsock connection not supported on current platform");
}

#[cfg(unix)]
fn listen_vsock(state: &mut AgentState) -> anyhow::Result<()> {
    let listener =
        vsock::VsockListener::bind_with_cid_port(3, 52).context("Failed to bind vsocket")?;

    for stream in listener.incoming() {
        let stream = stream.context("connect error")?;
        eprintln!("[agent] client connected: {:?}", stream);
    
        let writer = stream.try_clone().context("error cloning stream")?;
        let reader = BufReader::new(stream);

        match handle_connection_rpc(state, reader, writer) {
            Err(e) => eprintln!("[agent] client error: {}", e),
            Ok(false) => eprintln!("[agent] client disconnected"),
            Ok(true) => {
                eprintln!("[agent] exiting");
                break;
            }
        }
    }

    Ok(())
}

fn handle_connection_rpc<R, W>(
    state: &mut AgentState,
    mut reader: R,
    mut writer: W,
) -> anyhow::Result<bool>
where
    R: BufRead,
    W: Write,
{
    let mut request_id = 0;
    let mut buf = vec![];
    while state.exit.is_none() && reader.read_until(b'\n', &mut buf).is_ok() {
        state.reap_dead();

        let result = match serde_json::from_slice::<IpcWrapper<Request>>(&buf) {
            Ok(request) => {
                request_id = request.id;
                state.handle_request(request.body)
            }
            Err(err) => {
                request_id += 1;
                Err(anyhow::format_err!("{}", err))
            }
        };
        buf.clear();

        serde_json::to_writer(&mut std::io::Cursor::new(&mut buf), &IpcWrapper {
            id: request_id,
            body: agent::map_response(result),
        })
        .context("failed to encode response")?;
        buf.push(b'\n');
        writer.write_all(&buf).context("failed to send response")?;
        buf.clear();
    }

    match state.exit {
        Some(Exit::RestartAgent) => {
            state.kill_all()?;
            Ok(true)
        }
        Some(Exit::Shutdown) => {
            eprintln!("[agent] shutdown");
            state.kill_all()?;
            shutdown_vm()?;
            Ok(true)
        }
        None => Ok(false),
    }
}

fn shutdown_vm() -> anyhow::Result<()> {
    let result = process::Command::new("reboot").spawn().context("failed to run `reboot`")?.wait();
    match result {
        Ok(status) if status.success() => Ok(()),
        _ => anyhow::bail!("Failed to run reboot command: {:?}", result),
    }
}
