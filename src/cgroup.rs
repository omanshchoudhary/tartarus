use crate::config::Limits;
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

        fs::create_dir_all(&cgroup_path)?;

        Self::write_file(&cgroup_path.join("memory.max"), &limits.memory)?;
        // without this the limit leaks: pages the container exceeds with are pushed to host swap
        let swap_max = cgroup_path.join("memory.swap.max");
        if swap_max.exists() {
            Self::write_file(&swap_max, "0")?;
        }
        Self::write_file(&cgroup_path.join("pids.max"), &limits.pids.to_string())?;
        let quota = (limits.cpus * 100_000.0) as u64;
        let cpu_limit_str = format!("{} 100000", quota);
        Self::write_file(&cgroup_path.join("cpu.max"), &cpu_limit_str)?;

        Ok(Self { path: cgroup_path })
    }

    pub fn add_process(&self, pid: i32) -> anyhow::Result<()> {
        Self::write_file(&self.path.join("cgroup.procs"), &pid.to_string())
    }

    pub fn cleanup(self) -> anyhow::Result<()> {
        if self.path.exists() {
            fs::remove_dir(&self.path)?;
        }
        Ok(())
    }

    fn write_file(path: &Path, content: &str) -> anyhow::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }
}
