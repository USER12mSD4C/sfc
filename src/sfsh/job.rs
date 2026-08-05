use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::collections::HashMap;

pub struct Job {
    pub id: usize,
    pub pgid: Pid,
    pub cmd: String,
    pub done: bool,
    pub stopped: bool,
    pub status: i32,
}

pub struct JobTable {
    pub jobs: HashMap<usize, Job>,
    pub last_id: usize,
    pub current_pgid: Option<Pid>,
}

impl JobTable {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
            last_id: 0,
            current_pgid: None,
        }
    }

    pub fn add(&mut self, pgid: Pid, cmd: String) -> usize {
        self.last_id += 1;
        let id = self.last_id;
        self.jobs.insert(
            id,
            Job {
                id,
                pgid,
                cmd,
                done: false,
                stopped: false,
                status: 0,
            },
        );
        id
    }

    pub fn reap(&mut self) -> Vec<(usize, i32)> {
        let mut done = Vec::new();
        let ids: Vec<usize> = self.jobs.keys().copied().collect();
        for id in ids {
            if let Some(job) = self.jobs.get(&id) {
                if job.done {
                    done.push((id, job.status));
                }
            }
        }
        for (id, _) in &done {
            self.jobs.remove(id);
        }
        done
    }

    pub fn foreground(&mut self, id: usize, _shell_pgid: Pid) -> Option<i32> {
        let job = self.jobs.get(&id)?;
        let pgid = job.pgid;
        self.current_pgid = Some(pgid);
        unsafe {
            libc::tcsetpgrp(0, pgid.as_raw());
        }
        kill(pgid, Signal::SIGCONT).ok();
        None
    }

    pub fn background(&mut self, id: usize) -> Option<()> {
        let job = self.jobs.get_mut(&id)?;
        job.stopped = false;
        kill(job.pgid, Signal::SIGCONT).ok();
        Some(())
    }

    pub fn mark_done(&mut self, id: usize, status: i32) {
        if let Some(job) = self.jobs.get_mut(&id) {
            job.done = true;
            job.status = status;
        }
    }
}
