//! Render job state machine + registry.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct JobSnapshot {
    pub job_id: String,
    pub composition_id: String,
    pub state: JobState,
    pub width: u32,
    pub height: u32,
    pub duration_sec: f32,
    pub fps: u32,
    pub total_frames: u32,
    pub current_frame: u32,
    pub step: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
}

struct JobInner {
    snapshot: JobSnapshot,
}

#[derive(Clone)]
pub struct JobRegistry {
    inner: Arc<Mutex<HashMap<String, JobInner>>>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn create(
        &self,
        composition_id: String,
        width: u32,
        height: u32,
        duration_sec: f32,
        fps: u32,
    ) -> String {
        let job_id = format!("job-{}", uuid::Uuid::new_v4().simple());
        let total_frames = (duration_sec * fps as f32).ceil() as u32;
        let snapshot = JobSnapshot {
            job_id: job_id.clone(),
            composition_id,
            state: JobState::Queued,
            width,
            height,
            duration_sec,
            fps,
            total_frames,
            current_frame: 0,
            step: "queued".into(),
            started_at: now_unix(),
            finished_at: None,
            error: None,
        };
        self.inner
            .lock()
            .await
            .insert(job_id.clone(), JobInner { snapshot });
        job_id
    }

    pub async fn snapshot(&self, job_id: &str) -> Option<JobSnapshot> {
        self.inner
            .lock()
            .await
            .get(job_id)
            .map(|j| j.snapshot.clone())
    }

    pub async fn set_state(&self, job_id: &str, state: JobState) {
        if let Some(j) = self.inner.lock().await.get_mut(job_id) {
            j.snapshot.state = state;
            if matches!(
                state,
                JobState::Succeeded | JobState::Failed | JobState::Cancelled
            ) {
                j.snapshot.finished_at = Some(now_unix());
            }
        }
    }

    pub async fn set_progress(&self, job_id: &str, current: u32, step: String) {
        if let Some(j) = self.inner.lock().await.get_mut(job_id) {
            j.snapshot.current_frame = current;
            j.snapshot.step = step;
        }
    }

    pub async fn set_error(&self, job_id: &str, error: String) {
        if let Some(j) = self.inner.lock().await.get_mut(job_id) {
            j.snapshot.error = Some(error);
            j.snapshot.state = JobState::Failed;
            j.snapshot.finished_at = Some(now_unix());
        }
    }

    /// Returns true if job existed and was cancellable.
    pub async fn cancel(&self, job_id: &str) -> bool {
        if let Some(j) = self.inner.lock().await.get_mut(job_id) {
            if matches!(j.snapshot.state, JobState::Queued | JobState::Running) {
                j.snapshot.state = JobState::Cancelled;
                j.snapshot.finished_at = Some(now_unix());
                return true;
            }
        }
        false
    }
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
