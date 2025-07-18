use crate::diagnostics::{FileWatcher, FileChangeEvent};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_file_watcher_basic() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    
    let mut watcher = FileWatcher::new().expect("Failed to create file watcher");
    watcher.watch(temp_path).expect("Failed to start watching");
    
    // Create a test file
    let test_file = temp_path.join("test.txt");
    fs::write(&test_file, "Hello, world!").expect("Failed to write test file");
    
    // Wait for the create event
    if let Some(event) = watcher.next_event_timeout(Duration::from_secs(2)).await {
        match event {
            FileChangeEvent::Created(path) => {
                // Use canonicalized paths for comparison to handle macOS /private prefix
                let canonical_event_path = path.canonicalize().unwrap_or(path.clone());
                let canonical_test_path = test_file.canonicalize().unwrap_or(test_file.clone());
                assert_eq!(canonical_event_path, canonical_test_path);
                println!("✅ File creation detected: {:?}", path);
            }
            _ => panic!("Expected create event, got: {:?}", event),
        }
    } else {
        panic!("No file change event received within timeout");
    }
    
    // Modify the file
    fs::write(&test_file, "Hello, modified world!").expect("Failed to modify test file");
    
    // Wait for the modify event
    if let Some(event) = watcher.next_event_timeout(Duration::from_secs(2)).await {
        match event {
            FileChangeEvent::Modified(path) => {
                // Use canonicalized paths for comparison to handle macOS /private prefix
                let canonical_event_path = path.canonicalize().unwrap_or(path.clone());
                let canonical_test_path = test_file.canonicalize().unwrap_or(test_file.clone());
                assert_eq!(canonical_event_path, canonical_test_path);
                println!("✅ File modification detected: {:?}", path);
            }
            _ => panic!("Expected modify event, got: {:?}", event),
        }
    } else {
        panic!("No file modification event received within timeout");
    }
}

#[test]
fn test_should_ignore_file() {
    assert!(FileWatcher::should_ignore_file(Path::new(".gitignore")));
    assert!(FileWatcher::should_ignore_file(Path::new("file.tmp")));
    assert!(FileWatcher::should_ignore_file(Path::new("file.swp")));
    assert!(FileWatcher::should_ignore_file(Path::new("file~")));
    assert!(FileWatcher::should_ignore_file(Path::new("target/debug/file")));
    assert!(FileWatcher::should_ignore_file(Path::new("node_modules/package/file")));
    assert!(FileWatcher::should_ignore_file(Path::new(".git/HEAD")));
    
    assert!(!FileWatcher::should_ignore_file(Path::new("src/main.rs")));
    assert!(!FileWatcher::should_ignore_file(Path::new("README.md")));
    assert!(!FileWatcher::should_ignore_file(Path::new("Cargo.toml")));
}