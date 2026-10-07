//! Runs the slicer's requests on a background thread.

use laserline_core::scan::{self, Progress, Scan};
use laserline_core::slicer::{SliceRequest, Slicer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

struct Job {
    id: u64,
    cancel: Arc<AtomicBool>,
    progress: Arc<Progress>,
    rx: mpsc::Receiver<(Result<Scan, String>, f64)>,
}

/// One slicing thread at a time; starting a new request cancels the previous one.
#[derive(Default)]
pub struct SliceWorker {
    job: Option<Job>,
}

impl SliceWorker {
    /// Runs `req` on a new thread, cancelling the running slice.
    pub fn spawn(&mut self, req: SliceRequest) {
        self.cancel();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Progress::new(0));
        let (tx, rx) = mpsc::channel();
        let (c, p) = (cancel.clone(), progress.clone());
        let id = req.id;
        std::thread::spawn(move || {
            let t = Instant::now();
            let result = scan::slice(&req.mesh, &req.settings, &req.name, &c, &p);
            let _ = tx.send((result, t.elapsed().as_secs_f64()));
        });
        self.job = Some(Job { id, cancel, progress, rx });
    }

    /// Cancels the running slice, if any.
    pub fn cancel(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Progress of the running slice in 0..=1.
    pub fn progress(&self) -> Option<f32> {
        self.job.as_ref().map(|j| j.progress.load(Ordering::Relaxed) as f32 / 1000.0)
    }

    /// Passes a finished slice to `slicer`, then starts what the slicer requests.
    /// Returns a new scan to show.
    pub fn poll(&mut self, slicer: &mut Slicer, now_s: f64) -> Option<Scan> {
        // The slicer dropped the job (e.g. the model was replaced by a loaded scan).
        if self.job.as_ref().is_some_and(|j| slicer.running() != Some(j.id)) {
            self.cancel();
        }
        let mut done = None;
        if let Some(job) = &self.job {
            let outcome = match job.rx.try_recv() {
                Ok(r) => Some(r),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some((Err("slicing failed unexpectedly".into()), 0.0)),
            };
            if let Some((result, took)) = outcome {
                let id = job.id;
                self.job = None;
                done = slicer.finish(id, result, took);
            }
        }
        if let Some(req) = slicer.poll(now_s) {
            self.spawn(req);
        }
        done
    }
}
