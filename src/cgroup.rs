use crate::config::Limits;
use anyhow::Context;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Cgroup {
    path: PathBuf,
}

impl Cgroup {
    // Get the host cgroup path where the Tartarus runtime can create container sub-cgroups.
    fn get_base_path() -> anyhow::Result<PathBuf> {
        let content = fs::read_to_string("/proc/self/cgroup")?;

        for line in content.lines() {
            let parts: Vec<&str> = line.splitn(3, ':').collect();
            if parts.len() == 3 {
                let relative_path = parts[2];

                // Truncate right after user@<UID>.service
                if let Some(pos) = relative_path.find(".service") {
                    let end_idx = pos + ".service".len();
                    let delegated_subpath = &relative_path[..end_idx];

                    return Ok(PathBuf::from("/sys/fs/cgroup")
                        .join(delegated_subpath.trim_start_matches('/')));
                }
            }
        }

        Ok(PathBuf::from("/sys/fs/cgroup"))
    }
    pub fn create(limits: &Limits) -> anyhow::Result<Self> {
        let base_path = Self::get_base_path()?;
        let folder_name = format!("tartarus-{}", std::process::id());
        let cgroup_path = base_path.join(folder_name);

        // pids are recycled, so a group left by a killed run could carry stale limits into this one
        if cgroup_path.exists() {
            fs::remove_dir(&cgroup_path).with_context(|| {
                format!("stale cgroup in the way: {}", cgroup_path.display())
            })?;
        }
        fs::create_dir(&cgroup_path)
            .with_context(|| format!("create cgroup {}", cgroup_path.display()))?;

        let cgroup = Self { path: cgroup_path };

        cgroup.write_limit("memory.max", &limits.memory)?;
        // without this the limit leaks: pages the container exceeds with are pushed to host swap
        if cgroup.path.join("memory.swap.max").exists() {
            cgroup.write_limit("memory.swap.max", "0")?;
        }
        cgroup.write_limit("pids.max", &limits.pids.to_string())?;
        let quota = (limits.cpus * 100_000.0) as u64;
        cgroup.write_limit("cpu.max", &format!("{} 100000", quota))?;

        Ok(cgroup)
    }

    pub fn add_process(&self, pid: i32) -> anyhow::Result<()> {
        self.write_limit("cgroup.procs", &pid.to_string())
    }

    fn write_limit(&self, file: &str, content: &str) -> anyhow::Result<()> {
        let path = self.path.join(file);
        Self::write_file(&path, content)
            .with_context(|| format!("writing \"{}\" to {}", content, path.display()))
    }

    fn write_file(path: &Path, content: &str) -> anyhow::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }
}

// removing the group is tied to the value, so every early return and panic cleans up too
impl Drop for Cgroup {
    fn drop(&mut self) {
        if !self.path.exists() {
            return;
        }
        if let Err(err) = fs::remove_dir(&self.path) {
            eprintln!(
                "[Warning] could not remove cgroup {}: {}",
                self.path.display(),
                err
            );
        }
    }
}
