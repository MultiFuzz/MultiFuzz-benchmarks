//! Handle interactions with docker

use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Child, Command},
    time::SystemTime,
};

use anyhow::Context;

use crate::{utils::DeleteOnDrop, XShellExt};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DockerSource {
    /// The name of the docker image used for creating the root file system.
    pub tag: String,

    /// The directory containing the docker context used to build `image`.
    pub build_path: PathBuf,

    /// Paths to copy from the container to the file system.
    #[serde(default)]
    pub copy: Vec<PathBuf>,

    /// Empty folders to create in the file system.
    #[serde(default)]
    pub create_dirs: Vec<PathBuf>,
}

pub(crate) fn build_image(tag: &str, root: &Path, no_cache: bool) -> anyhow::Result<()> {
    let no_cache = no_cache.then(|| "--no-cache");
    let sh = xshell::Shell::new()?;
    xshell::cmd!(sh, "docker build -t {tag} {root} {no_cache...}").trace_cmd().run()?;
    Ok(())
}

/// Get the size of a docker image
pub(crate) fn get_image_size(config: &DockerSource) -> anyhow::Result<u64> {
    let tag = &config.tag;
    let sh = xshell::Shell::new()?;
    let output = xshell::cmd!(sh, "docker image inspect {tag} --format='{{.Size}}'").output()?;

    if !output.status.success() {
        anyhow::bail!(
            "error inspecting size of docker image: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let size = String::from_utf8(output.stdout)
        .map_err(anyhow::Error::msg)
        .and_then(|x| x.trim().parse::<u64>().map_err(anyhow::Error::msg))
        .context("error parsing image size")?;

    Ok(size)
}

/// Get the time the docker image was created at.
pub(crate) fn get_creation_time(config: &DockerSource) -> anyhow::Result<SystemTime> {
    let sh = xshell::Shell::new()?;
    let tag = &config.tag;
    let output = xshell::cmd!(sh, "docker image inspect {tag} --format='{{.Created}}'").output()?;

    if !output.status.success() {
        anyhow::bail!(
            "error inspecting creation date of docker image: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let time = String::from_utf8(output.stdout)
        .map_err(anyhow::Error::msg)
        .and_then(|x| {
            time::OffsetDateTime::parse(&x.trim(), &time::format_description::well_known::Rfc3339)
                .map_err(anyhow::Error::msg)
        })
        .context("error parsing image creation date")?;

    Ok(time.into())
}

struct CopyState<'a> {
    config: &'a DockerSource,
    container: Container,
    root: &'a Path,
}

/// Copy the contents of a docker container to a target directory.
pub(crate) fn copy_image(config: &DockerSource, dst_root: &Path) -> anyhow::Result<()> {
    let container = Container::create(&config.tag, &[])?;

    let mut state = CopyState { config, container, root: dst_root };

    copy_files(&state)?;
    state.container.remove()?;

    // Create any mounted folders
    let sh = xshell::Shell::new()?;
    for dir in &state.config.create_dirs {
        let path = state.root.join(dir);
        xshell::cmd!(sh, "mkdir {path}").trace_cmd().run()?;
    }

    Ok(())
}

pub enum MountType {
    Bind,
    #[allow(unused)]
    Volume,
    #[allow(unused)]
    TmpFs,
}

impl std::fmt::Display for MountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bind => f.write_str("bind"),
            Self::Volume => f.write_str("volume"),
            Self::TmpFs => f.write_str("tmpfs"),
        }
    }
}

pub struct Mount {
    type_: MountType,
    source: String,
    destination: String,
}

impl Mount {
    pub fn to_arg(&self) -> String {
        format!("type={},source={},destination={}", self.type_, self.source, self.destination)
    }
}

pub struct Container {
    name: String,
    active: bool,
    removed: bool,
}

impl Drop for Container {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

impl Container {
    pub fn create(image: &str, mounts: &[Mount]) -> anyhow::Result<Self> {
        let mut cmd = Command::new("docker");

        cmd.args(["create", image]);
        for mount in mounts {
            cmd.args(["--mount", &mount.to_arg()]);
        }

        Ok(Self { name: run_with_output(cmd)?, active: false, removed: false })
    }

    pub fn remove(&mut self) -> anyhow::Result<()> {
        if self.removed {
            return Ok(());
        }

        let name = &self.name;

        if self.active {
            let sh = xshell::Shell::new()?;
            xshell::cmd!(sh, "docker stop -t 1 {name}").run().context("failed to stop container")?;
            self.active = false;
        }

        let sh = xshell::Shell::new()?;
        xshell::cmd!(sh, "docker rm {name}").run().context("failed to remove container")?;
        self.removed = true;

        Ok(())
    }

    pub fn run_detached(
        image: &str,
        mounts: &[Mount],
        args: &[impl AsRef<OsStr>],
    ) -> anyhow::Result<Self> {
        let mut cmd = Command::new("docker");

        let (uid, gid) = get_uid_gid();
        cmd.args(["run", "-u", &format!("{uid}:{gid}"), "-d"]);
        for mount in mounts {
            cmd.args(["--mount", &mount.to_arg()]);
        }
        cmd.arg(image);
        cmd.args(args);
        Ok(Self { name: run_with_output(cmd)?, removed: false, active: true })
    }

    pub fn attach_command(&self) -> Command {
        let mut cmd = Command::new("docker");
        cmd.args(["attach", self.name.as_str()]);
        cmd
    }
}

#[cfg(unix)]
fn get_uid_gid() -> (u32, u32) {
    // Safety: these functions are safe to call.
    unsafe { (libc::getuid(), libc::getgid()) }
}

#[cfg(not(unix))]
fn get_uid_guid() -> (u32, u32) {
    (1000, 1000)
}


fn run_with_output(mut cmd: Command) -> anyhow::Result<String> {
    tracing::info!("Running: {cmd:?}");
    let output = cmd.output()?;
    match output.status.success() {
        true => Ok(String::from_utf8(output.stdout)?.trim().to_owned()),
        false => {
            anyhow::bail!("{cmd:?} failed with {}", String::from_utf8_lossy(&output.stderr))
        }
    }
}

fn copy_files(state: &CopyState) -> anyhow::Result<()> {
    let tmp_path = std::env::temp_dir().join("bench-harness-docker-extract");
    let handle = DeleteOnDrop(Some(tmp_path.clone()));

    for file in &state.config.copy {
        let file: &Path = file.as_ref();
        // @fixme: docker cp seems to fail sometimes when directly copying it to the target folder,
        // instead we pipe the output to a file and use tar to perform the extraction.
        let tmp_file = std::fs::File::create(&tmp_path)
            .context("failed to create temporary file for copying")?;
        let output = std::process::Command::new("docker")
            .arg("cp")
            .arg(format!("{}:/{}", state.container.name, file.display()))
            .arg("-")
            .stdout(tmp_file)
            .output()
            .context("error running docker cp")?;

        if !output.status.success() {
            anyhow::bail!("error running docker cp: {}", String::from_utf8_lossy(&output.stderr));
        }

        // Succeeded creating tar file, now extract it.
        let mut archive = tar::Archive::new(std::fs::File::open(&tmp_path)?);
        archive.set_preserve_permissions(true);
        archive.unpack(state.root).context("error unpacking archive")?;
    }

    drop(handle);
    Ok(())
}

pub struct DockerConfig {
    pub image: String,
    pub workdir: PathBuf,
    pub mounts: Vec<(PathBuf, PathBuf)>,
}

pub struct Worker {
    pub api_socket: PathBuf,
    #[allow(unused)] // Currently unused, but we could clean up old directories on exit.
    workdir: PathBuf,
    container: Container,
    process: Option<Child>,
}

impl Worker {
    pub fn wait_for_exit_timeout(mut self, timeout: std::time::Duration) -> anyhow::Result<()> {
        let mut process =
            self.process.take().ok_or_else(|| anyhow::format_err!("docker exited"))?;

        // Drop stdin to avoid deadlocks if the child is reading from stdin.
        drop(process.stdin.take());

        match crate::utils::wait_for_process_timeout(&mut process, timeout)? {
            None => anyhow::bail!("VM timed out after: {} seconds", timeout.as_secs()),
            Some(status) if !status.success() => {
                anyhow::bail!("VM exited with error: {status:?}")
            }
            Some(_) => {}
        };

        self.container.remove()
    }
}

pub(crate) fn spawn_docker_worker(id: String, config: &DockerConfig) -> anyhow::Result<Worker> {
    let workdir = config.workdir.join(&id);
    let api_socket = workdir.join("api.socket");
    crate::utils::prepare_workdir(&api_socket, &workdir, true, true)?;

    let mut mounts = vec![Mount {
        type_: MountType::Bind,
        source: workdir.canonicalize()?.to_str().unwrap().to_owned(),
        destination: "/var".into(),
    }];
    mounts.extend(config.mounts.iter().map(|(source, destination)| Mount {
        type_: MountType::Bind,
        source: source.canonicalize().unwrap().to_str().unwrap().to_owned(),
        destination: destination.to_str().unwrap().to_owned(),
    }));

    let container = Container::run_detached(&config.image, &mounts, &[
        "/bin/agent",
        "-u",
        "/var/api.socket",
    ])?;

    let mut attach_cmd = container.attach_command();
    crate::utils::redirect_stdio(&mut attach_cmd, &workdir)?;
    let process = Some(attach_cmd.spawn().with_context(|| format!("Failed to run docker"))?);

    Ok(Worker { container, api_socket, workdir, process })
}

pub(crate) fn prepare_instances(
    config: &crate::Config,
) -> anyhow::Result<HashMap<String, DockerConfig>> {
    let mut instances = HashMap::new();
    for (name, docker_config) in &config.data.docker {
        build_image(&name, &docker_config.build_path, false)?;

        let mut mounts = vec![];
        for mount in &docker_config.mount {
            mounts.push((copy_to_cache_dir(config, mount)?, mount.name.clone().into()));
        }

        instances.insert(name.clone(), DockerConfig {
            workdir: config.cache.dir.join(format!("{name}-workdir")),
            image: name.clone(),
            mounts,
        });
    }
    Ok(instances)
}

fn copy_to_cache_dir(
    config: &crate::config::Config,
    mount: &crate::config::DriveConfig,
) -> anyhow::Result<PathBuf> {
    use crate::image_builder::SourceKind;

    let path = config.cache.dir.join(&mount.image);
    if path.exists() {
        return Ok(path);
    }
    let image = config
        .data
        .images
        .get(&mount.image)
        .ok_or_else(|| anyhow::format_err!("unknown image: {}", mount.image))?;

    let host_src = match &image.kind {
        SourceKind::Docker(_) => anyhow::bail!("docker image not supported for docker mounts"),
        SourceKind::Host(files) => files,
    };

    for entry in &host_src.paths {
        crate::image_builder::utils::copy_into(&entry.src, &path.join(&entry.dst))?;
    }

    Ok(path)
}
