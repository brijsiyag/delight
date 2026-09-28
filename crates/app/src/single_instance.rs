//! One Delight at a time: a lock file holding the running instance's process id.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read as _, Seek as _, Write as _};
use std::path::Path;

use anyhow::{Context as _, Result};

/// Held while Delight runs; the lock is released when it drops (or the process ends).
pub struct InstanceLock {
    _file: File,
}

pub enum Acquired {
    /// This is the only Delight: keep the lock for as long as it runs.
    Locked(InstanceLock),
    /// Another Delight holds the lock; its process id, if it wrote one.
    Running { pid: Option<u32> },
}

/// Take the lock at `path` (creating it and its folder) and write this process's id
/// into it, or report the Delight that holds it.
pub fn acquire(path: &Path) -> Result<Acquired> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)
            .with_context(|| format!("creating {}", folder.display()))?;
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => {
            file.set_len(0)?;
            file.rewind()?;
            write!(file, "{}", std::process::id())?;
            Ok(Acquired::Locked(InstanceLock { _file: file }))
        }
        Err(TryLockError::WouldBlock) => {
            let mut pid = String::new();
            file.read_to_string(&mut pid)?;
            Ok(Acquired::Running {
                pid: pid.trim().parse().ok(),
            })
        }
        Err(TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("locking {}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_instance_finds_the_first_and_its_pid() {
        let path = std::env::temp_dir()
            .join(format!("delight-instance-test-{}", std::process::id()))
            .join("delight.lock");
        let first = acquire(&path).unwrap();
        assert!(matches!(first, Acquired::Locked(_)));
        let second = acquire(&path).unwrap();
        assert!(
            matches!(second, Acquired::Running { pid: Some(pid) } if pid == std::process::id())
        );

        // Once the first lets go, the next one gets the lock.
        drop(first);
        assert!(matches!(acquire(&path).unwrap(), Acquired::Locked(_)));
    }
}
