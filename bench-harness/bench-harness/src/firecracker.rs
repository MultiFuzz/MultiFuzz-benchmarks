use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
};

use anyhow::Context;

use crate::{
    config::{self, Config, MountKind},
    setup, utils,
};

#[derive(Clone)]
pub(crate) struct VmConfig {
    /// Path to the firecracker binary.
    pub bin: PathBuf,

    /// The amount of time (in seconds) we should sleep before attempting to connect to the agent.
    pub boot_delay_sec: u64,

    /// Bytes of entropy to inject into the kernel.
    pub kernel_entropy: Option<Vec<u32>>,

    /// Whether to recreate the working directory for the VM.
    pub recreate_work_dir: bool,

    /// Boot configuration for the Vm.
    pub boot: BootSource,

    /// Machine configuration for the Vm.
    pub machine: MachineConfig,

    /// The root file system to mount in the VM.
    pub rootfs: DriveConfig,

    /// Additional file systems that should be mounted in the VM.
    pub drives: Vec<DriveConfig>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct BootSource {
    /// Path to kernel image
    pub kernel_image_path: PathBuf,

    /// Arguments to boot the kernel withb
    pub boot_args: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct MachineConfig {
    /// Enable hyperthreading in the Vm.
    pub smt: bool,

    /// The amount of memory to reserve for the Vm.
    pub mem_size_mib: u64,

    /// The number of cores supported by the Vm.
    pub vcpu_count: u8,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self { smt: false, mem_size_mib: 512, vcpu_count: 1 }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DriveConfig {
    pub name: String,
    pub path: PathBuf,
    pub mount: MountKind,
}

struct FirecrakerInstance {
    process: std::process::Child,
}

impl Drop for FirecrakerInstance {
    fn drop(&mut self) {
        let _ = self.process.kill();
    }
}

pub(crate) struct ActiveVm {
    pub workdir: PathBuf,
    drives: Vec<Drive>,
    api: curl::easy::Easy,
    vsock_path: PathBuf,
    instance: Option<FirecrakerInstance>,
}

impl ActiveVm {
    pub fn wait_for_exit(mut self) -> anyhow::Result<()> {
        let mut instance = self.instance.take().ok_or_else(|| anyhow::format_err!("VM exited"))?;

        let _stdin = instance.process.stdin.take();
        let exit = instance.process.wait()?;

        if !exit.success() {
            anyhow::bail!("VM exited with error: {:?}", exit)
        }
        Ok(())
    }

    pub fn wait_for_exit_timeout(mut self, timeout: std::time::Duration) -> anyhow::Result<()> {
        let mut instance = self.instance.take().ok_or_else(|| anyhow::format_err!("VM exited"))?;

        // Drop stdin to avoid deadlocks if the child is reading from stdin.
        drop(instance.process.stdin.take());

        match crate::utils::wait_for_process_timeout(&mut instance.process, timeout)? {
            None => anyhow::bail!("VM timed out after: {} seconds", timeout.as_secs()),
            Some(status) if !status.success() => {
                anyhow::bail!("VM exited with error: {:?}", status)
            }
            Some(_) => Ok(()),
        }
    }

    pub fn add_drive(&mut self, config: &DriveConfig, is_root_device: bool) -> anyhow::Result<()> {
        if !config.path.exists() {
            // Error early if the path to the drive does not exist -- the drive could still be
            // deleted in between the point where we actually run the VM, however this
            // is rare and we check here to produce a better error message and avoid
            // doing extra work.
            anyhow::bail!(
                "Failed to configure: {}, {} does not exist",
                config.name,
                config.path.display()
            );
        }

        let (is_read_only, path_on_host) = match config.mount {
            MountKind::ReadOnly => (true, config.path.clone()),
            MountKind::Duplicate => {
                let copy_path = self.workdir.join(format!("{}.ext4", config.name));
                if !copy_path.exists() {
                    std::fs::copy(&config.path, &copy_path).with_context(|| {
                        format!(
                            "error copying {} to {}",
                            config.path.display(),
                            copy_path.display()
                        )
                    })?;
                }
                (false, PathBuf::from(copy_path).canonicalize()?)
            }
            MountKind::ReuseDuplicate => {
                let copy_path = self.workdir.join(format!("{}.ext4", config.name));
                if !copy_path.exists() {
                    anyhow::bail!(
                        "Attempting to reuse: {} but file does not exist",
                        copy_path.display()
                    );
                }
                (false, PathBuf::from(copy_path).canonicalize()?)
            }
            MountKind::InPlace => (false, config.path.clone()),
        };

        self.drives.push(Drive {
            drive_id: config.name.clone(),
            path_on_host,
            is_root_device,
            is_read_only,
        });
        Ok(())
    }

    fn send_config(&mut self, config: &VmConfig) -> anyhow::Result<()> {
        put::<_, ()>(&mut self.api, "http://localhost/boot-source", &config.boot)
            .context("Error sending boot config")?;

        put::<_, ()>(&mut self.api, "http://localhost/machine-config", &config.machine)
            .context("Error sending machine config")?;

        for drive in &self.drives {
            let path = format!("http://localhost/drives/{}", drive.drive_id);
            put::<_, ()>(&mut self.api, &path, drive)
                .with_context(|| format!("Error configuring drive: {}", drive.drive_id))?;
        }

        put::<_, ()>(&mut self.api, "http://localhost/vsock", &Vsock {
            guest_cid: 3,
            uds_path: self.vsock_path.clone(),
        })
        .context("Error configuring vsock")?;

        put::<_, ()>(&mut self.api, "http://localhost/actions", &Action {
            action_type: "InstanceStart".into(),
        })
        .context("Error starting instance")?;

        Ok(())
    }
}

pub(crate) fn spawn_vm(
    id: String,
    config: &VmConfig,
    interactive: bool,
) -> anyhow::Result<ActiveVm> {
    let workdir = std::env::temp_dir().join("bench-harness").join(&id);
    let api_socket = workdir.join("firecracker-api.socket");
    utils::prepare_workdir(&api_socket, &workdir, config.recreate_work_dir, false)?;

    // Start the firecracker subprocess
    let mut command = std::process::Command::new(&config.bin);
    command.arg("--api-sock").arg(&api_socket);

    if !interactive {
        crate::utils::redirect_stdio(&mut command, &workdir)?;
    }

    let instance = FirecrakerInstance {
        process: command
            .spawn()
            .with_context(|| format!("Failed to start `{}`", config.bin.display()))?,
    };

    // Wait for the API server to be ready
    std::thread::sleep(std::time::Duration::from_millis(100));
    while !api_socket.exists() {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Connect the API server, and configure the VM
    tracing::debug!("Connecting to api server using {}", api_socket.display());
    let mut api = curl::easy::Easy::new();
    curl::easy::Easy::unix_socket_path(&mut api, Some(&api_socket))
        .with_context(|| format!("error connecting to api socket ({})", api_socket.display()))?;

    // Configure vsocket
    let vsock_path = workdir.join("vm.vsock");
    if let Err(e) = std::fs::remove_file(&vsock_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            anyhow::bail!("error removing: {}, {}", vsock_path.display(), e);
        }
    }

    let mut vm = ActiveVm { workdir, api, instance: Some(instance), drives: vec![], vsock_path };

    vm.add_drive(&config.rootfs, true)?;
    for drive in &config.drives {
        vm.add_drive(drive, false)?;
    }

    vm.send_config(&config)?;

    let sleep = config.boot_delay_sec;
    tracing::debug!("VM started, waiting {} seconds for boot...", sleep);
    std::thread::sleep(std::time::Duration::from_secs(sleep));

    Ok(vm)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Drive {
    drive_id: String,
    path_on_host: PathBuf,
    is_root_device: bool,
    is_read_only: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Action {
    action_type: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Vsock {
    guest_cid: usize,
    uds_path: PathBuf,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum FirecrakerResult<T> {
    Ok(T),
    Error { error: String },
    Fault { fault_message: String },
}

fn put<I, O>(api: &mut curl::easy::Easy, url: &str, data: &I) -> anyhow::Result<Option<O>>
where
    I: serde::Serialize,
    O: serde::de::DeserializeOwned,
{
    let input = serde_json::ser::to_vec(&data)?;
    let mut output: Vec<u8> = vec![];

    let mut headers = curl::easy::List::new();
    headers.append("Content-Type: application/json")?;
    headers.append("Accept: application/json")?;

    api.http_headers(headers)?;
    api.custom_request("PUT")?;
    api.url(url)?;
    api.post_field_size(input.len() as u64)?;

    {
        let mut input = std::io::Cursor::new(&input);
        let mut output = std::io::Cursor::new(&mut output);

        let mut transfer = api.transfer();

        transfer.read_function(|buf| Ok(input.read(buf).unwrap()))?;
        transfer.write_function(|buf| Ok(output.write(buf).unwrap()))?;

        transfer.perform()?;
    }

    if output.is_empty() {
        return Ok(None);
    }

    let result: FirecrakerResult<O> = serde_json::from_slice(&output).with_context(|| {
        format!("failed to deserialize response: {:?}", String::from_utf8_lossy(&output))
    })?;

    match result {
        FirecrakerResult::Ok(value) => Ok(Some(value)),
        FirecrakerResult::Error { error } | FirecrakerResult::Fault { fault_message: error } => {
            Err(anyhow::format_err!("{}", error))
        }
    }
}

pub(crate) fn connect_to_vsock_agent(
    vm: &ActiveVm,
) -> anyhow::Result<Box<dyn agent_interface::client::Agent>> {
    #[cfg(unix)]
    {
        const MAX_RETRIES: usize = 3;
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

        let agent = agent_interface::client::retry(
            || {
                anyhow::ensure!(!crate::should_stop(), "task cancelled");
                Ok(connect_firecracker(&vm.vsock_path, 52))
            },
            MAX_RETRIES,
            RETRY_DELAY,
        )?;
        Ok(Box::new(agent))
    }

    #[cfg(not(unix))]
    {
        let _ = vm;
        anyhow::bail!("Unable to connect to agent on current platform")
    }
}

#[cfg(unix)]
fn connect_firecracker(
    path: &std::path::Path,
    port: u32,
) -> anyhow::Result<agent_interface::client::unix::UnixAgent> {
    tracing::debug!("Connecting to firecracker agent at: {}:{}", path.display(), port);
    let mut agent = agent_interface::client::unix::UnixAgent::connect(path)?;
    firecracker_handshake(&mut agent.reader, &mut agent.writer, port)?;
    Ok(agent)
}

#[cfg(unix)]
fn firecracker_handshake<R, W>(mut reader: R, mut writer: W, port: u32) -> anyhow::Result<()>
where
    R: std::io::Read,
    W: std::io::Write,
{
    tracing::debug!("Sending handshake");
    writer.write_all(format!("CONNECT {}\n", port).as_bytes())?;

    let mut buf = [0; 16];
    let n = reader.read(&mut buf)?;
    if n == 0 {
        anyhow::bail!("firecracker responded with empty string to CONNECT message");
    }

    let msg = &buf[..n];
    anyhow::ensure!(
        msg.starts_with(b"OK"),
        "Unexpected response from firecracker: {}",
        String::from_utf8_lossy(msg)
    );

    tracing::debug!("Successfully performed handshake");
    Ok(())
}

pub fn get_instance_config(config: &Config) -> anyhow::Result<HashMap<String, VmConfig>> {
    let firecracker_config = config
        .firecracker
        .as_ref()
        .ok_or_else(|| anyhow::format_err!("[firecracker] config missing"))?;
    let firecracker = setup::get_firecracker_path(firecracker_config, &config.cache)?;
    tracing::debug!("firecracker: {}", firecracker.display());

    let kernel_config = &firecracker_config.kernel;
    let kernel = setup::get_kernel_path(kernel_config, &config.cache)?;
    tracing::debug!("kernel: {}", kernel.display());

    let image_paths: HashMap<_, _> = config
        .data
        .images
        .iter()
        .map(|(name, _)| Ok((name, crate::image_builder::get_image_path(name, &config.cache)?)))
        .collect::<anyhow::Result<_>>()?;

    let mut instances = HashMap::new();
    for (name, instance) in &config.data.instances {
        let vm_config =
            build_instance(&firecracker, instance, kernel_config, &kernel, &image_paths)
                .with_context(|| format!("failed to build: {name}"))?;
        instances.insert(name.clone(), vm_config);
    }

    Ok(instances)
}

pub fn build_instance(
    firecracker: &PathBuf,
    instance: &config::Instance,
    kernel_config: &config::Kernel,
    kernel: &PathBuf,
    image_paths: &HashMap<&String, PathBuf>,
) -> anyhow::Result<VmConfig> {
    Ok(VmConfig {
        bin: firecracker.clone(),
        boot_delay_sec: instance.boot_delay_sec,
        recreate_work_dir: instance.recreate_workdir,
        kernel_entropy: kernel_config.entropy.clone(),
        boot: BootSource {
            kernel_image_path: kernel.clone(),
            boot_args: kernel_config.boot_args.clone(),
        },
        machine: instance.machine.clone(),
        rootfs: DriveConfig {
            name: instance.rootfs.name.clone(),
            path: image_paths
                .get(&instance.rootfs.image)
                .ok_or_else(|| {
                    anyhow::format_err!("failed to find rootfs image: {}", instance.rootfs.image)
                })?
                .clone(),
            mount: instance.rootfs.mount_as,
        },
        drives: instance
            .drives
            .iter()
            .map(|drive| {
                Ok(DriveConfig {
                    name: drive.name.clone(),
                    path: image_paths
                        .get(&drive.image)
                        .ok_or_else(|| {
                            anyhow::format_err!("failed to find drive: {}", drive.image)
                        })?
                        .clone(),
                    mount: drive.mount_as,
                })
            })
            .collect::<anyhow::Result<Vec<DriveConfig>>>()?,
    })
}

pub fn spawn_debug_vm(config: &VmConfig) -> anyhow::Result<()> {
    let vm = spawn_vm("vm-debug-data".into(), config, true)?;

    let mut agent = connect_to_vsock_agent(&vm)?;
    if let Some(entropy) = config.kernel_entropy.clone() {
        agent.send(agent_interface::Request::AddEntropy(entropy))?;
    }

    let pid = agent.spawn_task(
        agent_interface::RunCommand::from_cmd_string("/bin/bash -i")
            .unwrap()
            .stdin(agent_interface::Stdio::Inherit)
            .stdout(agent_interface::Stdio::Inherit)
            .stderr(agent_interface::Stdio::Inherit),
    )?;
    tracing::debug!("`/bin/bash` pid={pid}");

    vm.wait_for_exit()?;
    Ok(())
}

/// Builds all images used for VMs. This is not done as part of normal execution because it
/// currently requires root permissions (in order to mount disks).
pub fn build_images(config: &Config) -> anyhow::Result<()> {
    for (name, source) in &config.data.images {
        crate::image_builder::build_image(&name, &source, &config.cache)
            .with_context(|| format!("failed to build: {name}"))?;
    }
    Ok(())
}
