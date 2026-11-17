//! In-process demo tasks. No WSL, disk, installation or GPU operations.
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    Succeeded,
    Cancelled,
    Failed,
}
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub progress: u32,
    pub state: State,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    InvalidHandle,
    Capacity,
    Closed,
    Worker,
}

pub struct Task {
    progress: Arc<AtomicU32>,
    state: Arc<AtomicU32>,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Task {
    pub fn start(steps: u32) -> Result<Self, &'static str> {
        Self::start_demo(steps, 0).map_err(|_| "steps must be between 1 and 100")
    }
    pub fn start_demo(steps: u32, fail_at: u32) -> Result<Self, Error> {
        if !(1..=100).contains(&steps) || fail_at > steps {
            return Err(Error::InvalidInput);
        }
        let progress = Arc::new(AtomicU32::new(0));
        let state = Arc::new(AtomicU32::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (p, s, c) = (progress.clone(), state.clone(), cancelled.clone());
        let worker = thread::Builder::new()
            .name("wsx-diagnostic-task".into())
            .spawn(move || {
                for n in 1..=steps {
                    thread::sleep(Duration::from_millis(10));
                    if c.load(Ordering::Acquire) {
                        s.store(2, Ordering::Release);
                        return;
                    }
                    p.store(n, Ordering::Release);
                    if n == fail_at {
                        s.store(3, Ordering::Release);
                        return;
                    }
                }
                s.store(1, Ordering::Release);
            })
            .map_err(|_| Error::Worker)?;
        Ok(Self {
            progress,
            state,
            cancelled,
            worker: Some(worker),
        })
    }
    pub fn progress(&self) -> u32 {
        self.progress.load(Ordering::Acquire)
    }
    pub fn snapshot(&self) -> Snapshot {
        let state = match self.state.load(Ordering::Acquire) {
            0 => State::Running,
            1 => State::Succeeded,
            2 => State::Cancelled,
            _ => State::Failed,
        };
        Snapshot {
            state,
            progress: self.progress(),
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Inner {
    releasing: usize,
    next: u64,
    closed: bool,
    tasks: BTreeMap<u64, Task>,
}
pub struct Tasks(Mutex<Inner>, Condvar);
impl Default for Tasks {
    fn default() -> Self {
        Self(
            Mutex::new(Inner {
                releasing: 0,
                next: 1,
                closed: false,
                tasks: BTreeMap::new(),
            }),
            Condvar::new(),
        )
    }
}
impl Tasks {
    pub fn start(&self, steps: u32, fail_at: u32) -> Result<u64, Error> {
        let mut inner = self.0.lock().map_err(|_| Error::Worker)?;
        if inner.closed {
            return Err(Error::Closed);
        }
        if inner.tasks.len() + inner.releasing >= 16 {
            return Err(Error::Capacity);
        }
        let next = inner.next.checked_add(1).ok_or(Error::Capacity)?;
        let task = Task::start_demo(steps, fail_at)?;
        let id = inner.next;
        inner.next = next;
        inner.tasks.insert(id, task);
        Ok(id)
    }
    pub fn query(&self, id: u64) -> Result<Snapshot, Error> {
        let inner = self.0.lock().map_err(|_| Error::Worker)?;
        Ok(inner.tasks.get(&id).ok_or(Error::InvalidHandle)?.snapshot())
    }
    pub fn cancel(&self, id: u64) -> Result<(), Error> {
        let inner = self.0.lock().map_err(|_| Error::Worker)?;
        inner.tasks.get(&id).ok_or(Error::InvalidHandle)?.cancel();
        Ok(())
    }
    pub fn release(&self, id: u64) -> Result<(), Error> {
        let task = {
            let mut inner = self.0.lock().map_err(|_| Error::Worker)?;
            let task = inner.tasks.remove(&id).ok_or(Error::InvalidHandle)?;
            inner.releasing += 1;
            task
        };
        drop(task); // Join outside the registry lock; shutdown accounts for this release.
        let mut inner = self.0.lock().map_err(|_| Error::Worker)?;
        inner.releasing -= 1;
        self.1.notify_all();
        Ok(())
    }
    pub fn shutdown(&self) -> Result<(), Error> {
        let tasks = {
            let mut inner = self.0.lock().map_err(|_| Error::Worker)?;
            inner.closed = true;
            inner.releasing += 1;
            std::mem::take(&mut inner.tasks)
        };
        for task in tasks.values() {
            task.cancel();
        }
        drop(tasks);
        let mut inner = self.0.lock().map_err(|_| Error::Worker)?;
        inner.releasing -= 1;
        self.1.notify_all();
        while inner.releasing != 0 {
            inner = self.1.wait(inner).map_err(|_| Error::Worker)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds() {
        assert!(Task::start(0).is_err());
        assert!(Task::start(101).is_err());
        assert!(Task::start_demo(2, 3).is_err());
    }
    #[test]
    fn cancel_and_join() {
        let task = Task::start(100).unwrap();
        task.cancel();
        drop(task);
    }
    #[test]
    fn concurrent_progress_queries_complete() {
        let tasks = Arc::new(Tasks::default());
        let id = tasks.start(3, 0).unwrap();
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let tasks = tasks.clone();
                thread::spawn(move || {
                    let deadline = std::time::Instant::now() + Duration::from_secs(5);
                    let mut previous = 0;
                    loop {
                        let snapshot = tasks.query(id).unwrap();
                        assert!(snapshot.progress >= previous && snapshot.progress <= 3);
                        previous = snapshot.progress;
                        if snapshot.state == State::Succeeded {
                            break;
                        }
                        assert!(std::time::Instant::now() < deadline);
                        thread::yield_now();
                    }
                })
            })
            .collect();
        for reader in readers {
            reader.join().unwrap();
        }
        tasks.release(id).unwrap();
        assert_eq!(tasks.query(id).unwrap_err(), Error::InvalidHandle);
    }
    #[test]
    fn shutdown_blocks_new_tasks_and_retires_handles() {
        let tasks = Tasks::default();
        let id = tasks.start(100, 0).unwrap();
        tasks.shutdown().unwrap();
        tasks.shutdown().unwrap();
        assert_eq!(tasks.start(1, 0).unwrap_err(), Error::Closed);
        assert_eq!(tasks.query(id).unwrap_err(), Error::InvalidHandle);
    }
    #[test]
    fn capacity_is_bounded() {
        let tasks = Tasks::default();
        for _ in 0..16 {
            tasks.start(100, 0).unwrap();
        }
        assert_eq!(tasks.start(1, 0).unwrap_err(), Error::Capacity);
        tasks.shutdown().unwrap();
    }
}
