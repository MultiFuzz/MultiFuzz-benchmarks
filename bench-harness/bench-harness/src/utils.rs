use std::{path::PathBuf, time::Duration};

use anyhow::Context;
use serde::Deserialize;

#[derive(Default, Clone)]
pub struct Variables {
    vars: indexmap::IndexMap<String, String>,
}

impl Variables {
    pub fn insert(&mut self, key: String, value: String) {
        if !value.contains("{") {
            self.vars.insert(key, value);
            return;
        }

        let resolved_value = self.expand_vars(&value);
        self.vars.insert(key, resolved_value);
    }

    pub fn insert_all(&mut self, entries: impl Iterator<Item = (String, String)>) {
        entries.for_each(|(key, value)| self.insert(key, value));
    }

    // @fixme: clean up the code so we no longer need this.
    pub fn expand_vars(&self, mut input: &str) -> String {
        let mut expanded = String::new();

        while !input.is_empty() {
            let (before, key, after) = match split_template(input) {
                Some(value) => value,
                None => {
                    expanded.push_str(&input);
                    break;
                }
            };

            expanded.push_str(&before);

            match self.vars.get(key) {
                Some(value) => expanded.push_str(value),
                None => {
                    expanded.push('{');
                    expanded.push_str(key);
                    expanded.push('}');
                }
            }

            input = after;
        }

        expanded
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(|x| x.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> + '_ {
        self.vars.iter()
    }
}

fn split_template(input: &str) -> Option<(&str, &str, &str)> {
    let var_start = input.find('{')?;
    let (before, rest) = input.split_at(var_start);
    let var_end = rest.find('}')?;

    let key = &rest[1..var_end];
    let after = &rest[var_end + 1..];

    Some((before, key, after))
}

impl IntoIterator for Variables {
    type Item = (String, String);
    type IntoIter = <indexmap::IndexMap<String, String> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.vars.into_iter()
    }
}

pub(crate) struct KillOnDrop {
    pub child: std::process::Child,
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl KillOnDrop {
    #[allow(dead_code)]
    pub fn new(child: std::process::Child) -> Self {
        Self { child, done: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)) }
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        self.done.store(true, std::sync::atomic::Ordering::Release);
        let _ = self.child.kill();
    }
}

pub fn wait_for_process_timeout(
    handle: &mut std::process::Child,
    timeout: std::time::Duration,
) -> anyhow::Result<Option<std::process::ExitStatus>> {
    let mut remaining_time = timeout;
    loop {
        const SLEEP_TIME: std::time::Duration = std::time::Duration::from_millis(10);
        match handle.try_wait()? {
            Some(exit) => return Ok(Some(exit)),
            None => std::thread::sleep(SLEEP_TIME),
        }
        remaining_time = match remaining_time.checked_sub(SLEEP_TIME) {
            Some(timeout) => timeout,
            None => return Ok(None),
        }
    }
}

pub(crate) struct DeleteOnDrop(pub Option<std::path::PathBuf>);

impl DeleteOnDrop {
    pub(crate) fn finalize(mut self) {
        drop(self.0.take());
    }
}

impl Drop for DeleteOnDrop {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

pub(crate) fn parse_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    struct DurationSource;
    impl<'de> serde::de::Visitor<'de> for DurationSource {
        type Value = Duration;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("Duration")
        }

        fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Duration::from_secs_f64(v))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Duration::from_secs(v))
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            parse_duration_str(v)
                .ok_or_else(|| serde::de::Error::custom(format!("invalid time format: {v}")))
        }
    }

    deserializer.deserialize_any(DurationSource)
}

pub(crate) fn parse_duration_opt<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(transparent)]
    struct DurationWrapper(#[serde(deserialize_with = "parse_duration")] Duration);
    Option::<DurationWrapper>::deserialize(deserializer).map(|x| x.map(|wrapped| wrapped.0))
}

pub(crate) fn parse_duration_str(name: &str) -> Option<Duration> {
    if let Some(hours) = name
        .strip_suffix("hours")
        .or_else(|| name.strip_suffix("hour"))
        .or_else(|| name.strip_suffix("hrs"))
        .or_else(|| name.strip_suffix("hr"))
        .or_else(|| name.strip_suffix("h"))
    {
        return Some(Duration::from_secs_f64(hours.parse::<f64>().ok()? * 60.0 * 60.0));
    }
    else if let Some(mins) = name
        .strip_suffix("minutes")
        .or_else(|| name.strip_suffix("minute"))
        .or_else(|| name.strip_suffix("mins"))
        .or_else(|| name.strip_suffix("min"))
        .or_else(|| name.strip_suffix("m"))
    {
        return Some(Duration::from_secs_f64(mins.parse::<f64>().ok()? * 60.0));
    }
    else if let Some(seconds) = name
        .strip_suffix("seconds")
        .or_else(|| name.strip_suffix("second"))
        .or_else(|| name.strip_suffix("secs"))
        .or_else(|| name.strip_suffix("sec"))
        .or_else(|| name.strip_suffix("s"))
    {
        return Some(Duration::from_secs_f64(seconds.parse::<f64>().ok()?));
    }
    None
}

pub struct HumanReadableDuration(pub std::time::Duration);

impl std::fmt::Display for HumanReadableDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let seconds = self.0.as_secs();
        let minutes = seconds / 60;
        let hours = minutes / 60;
        let days = hours / 24;

        let seconds = seconds - (60 * minutes);
        let minutes = minutes - (60 * hours);
        let hours = hours - (24 * days);

        if days != 0 {
            f.write_fmt(format_args!("{days} days "))?;
        }
        if hours != 0 {
            f.write_fmt(format_args!("{hours} hours "))?;
        }
        if minutes != 0 {
            f.write_fmt(format_args!("{minutes} mins "))?;
        }
        if seconds != 0 {
            f.write_fmt(format_args!("{seconds} seconds"))?;
        }

        Ok(())
    }
}

pub fn prepare_workdir(
    api_socket: &PathBuf,
    workdir: &PathBuf,
    recreate_work_dir: bool,
    force: bool
) -> anyhow::Result<()> {
    if recreate_work_dir {
        // Remove a previous working directory -- Note: we check whether the api socket exists as a
        // level of protection against accidentally deleting a directory that wasn't created by a
        // previous execution.
        if api_socket.exists() || force {
            if let Err(e) = std::fs::remove_dir_all(workdir) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    anyhow::bail!("Error removing: {}, {e}", workdir.display());
                }
            }
        }
        if workdir.exists() {
            anyhow::bail!("workdir {} already exists", workdir.display());
        }
        std::fs::create_dir_all(workdir)
            .with_context(|| format!("failed to create workdir {}", workdir.display()))?;
    }

    if let Err(e) = std::fs::remove_file(api_socket) {
        if e.kind() != std::io::ErrorKind::NotFound {
            anyhow::bail!("Error removing: {}, {e}", api_socket.display());
        }
    }

    Ok(())
}

pub fn redirect_stdio(
    command: &mut std::process::Command,
    workdir: &PathBuf,
) -> anyhow::Result<()> {
    command.stdin(std::process::Stdio::null());

    let stdout_path = workdir.join("stdout");
    command.stdout(std::process::Stdio::from(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&stdout_path)
            .with_context(|| format!("failed to create: {}", stdout_path.display()))?,
    ));

    let stderr_path = workdir.join("stderr");
    command.stderr(std::process::Stdio::from(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&stderr_path)
            .with_context(|| format!("failed to create: {}", stderr_path.display()))?,
    ));
    Ok(())
}
