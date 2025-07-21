use crate::diagnostics::types::FileChangeEvent;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Result, Watcher};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<FileChangeEvent>,
}

impl FileWatcher {
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::channel(1000);
        
        let watcher = notify::recommended_watcher({
            let tx = tx.clone();
            move |res: notify::Result<Event>| {
                if let Ok(event) = res {
                    let change_event = Self::convert_event(event);
                    if let Some(change) = change_event {
                        // Use blocking send since we're in a sync callback
                        if let Err(_) = tx.blocking_send(change) {
                            eprintln!("Failed to send file change event");
                        }
                    }
                }
            }
        })?;
        
        Ok(Self {
            _watcher: watcher,
            receiver: rx,
        })
    }
    
    pub fn watch<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self._watcher.watch(path.as_ref(), RecursiveMode::Recursive)
    }
    
    pub async fn next_event(&mut self) -> Option<FileChangeEvent> {
        self.receiver.recv().await
    }
    
    pub async fn next_event_timeout(&mut self, timeout: Duration) -> Option<FileChangeEvent> {
        tokio::time::timeout(timeout, self.receiver.recv()).await.ok().flatten()
    }
    
    fn convert_event(event: Event) -> Option<FileChangeEvent> {
        match event.kind {
            EventKind::Create(_) => {
                if let Some(path) = event.paths.first() {
                    Some(FileChangeEvent::Created(path.clone()))
                } else {
                    None
                }
            }
            EventKind::Modify(_) => {
                if let Some(path) = event.paths.first() {
                    Some(FileChangeEvent::Modified(path.clone()))
                } else {
                    None
                }
            }
            EventKind::Remove(_) => {
                if let Some(path) = event.paths.first() {
                    Some(FileChangeEvent::Deleted(path.clone()))
                } else {
                    None
                }
            }
            EventKind::Other => {
                // Handle rename events
                if event.paths.len() == 2 {
                    Some(FileChangeEvent::Renamed {
                        from: event.paths[0].clone(),
                        to: event.paths[1].clone(),
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    
    pub fn should_ignore_file(path: &Path) -> bool {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Ignore common temporary and build files
            if name.starts_with('.') 
                || name.ends_with('~') 
                || name.ends_with(".tmp") 
                || name.ends_with(".swp") {
                return true;
            }
        }
        
        // Ignore common build directories
        if let Some(parent) = path.parent() {
            if let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
                if dir_name == "target" 
                    || dir_name == "node_modules" 
                    || dir_name == ".git" 
                    || dir_name == "build" {
                    return true;
                }
            }
        }
        
        false
    }
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new().expect("Failed to create file watcher")
    }
}