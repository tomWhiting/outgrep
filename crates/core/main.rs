/*!
The main entry point into ripgrep.
*/

use std::{io::Write, process::ExitCode};

use ignore::WalkState;

use crate::flags::{HiArgs, SearchMode};

#[macro_use]
mod messages;

mod flags;
mod haystack;
mod logger;
mod search;

mod diagnostics;

// Since Rust no longer uses jemalloc by default, ripgrep will, by default,
// use the system allocator. On Linux, this would normally be glibc's
// allocator, which is pretty good. In particular, ripgrep does not have a
// particularly allocation heavy workload, so there really isn't much
// difference (for ripgrep's purposes) between glibc's allocator and jemalloc.
//
// However, when ripgrep is built with musl, this means ripgrep will use musl's
// allocator, which appears to be substantially worse. (musl's goal is not to
// have the fastest version of everything. Its goal is to be small and amenable
// to static compilation.) Even though ripgrep isn't particularly allocation
// heavy, musl's allocator appears to slow down ripgrep quite a bit. Therefore,
// when building with musl, we use jemalloc.
//
// We don't unconditionally use jemalloc because it can be nice to use the
// system's default allocator by default. Moreover, jemalloc seems to increase
// compilation times by a bit.
//
// Moreover, we only do this on 64-bit systems since jemalloc doesn't support
// i686.
#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

/// Then, as it was, then again it will be.
fn main() -> ExitCode {
    match run(flags::parse()) {
        Ok(code) => code,
        Err(err) => {
            // Look for a broken pipe error. In this case, we generally want
            // to exit "gracefully" with a success exit code. This matches
            // existing Unix convention. We need to handle this explicitly
            // since the Rust runtime doesn't ask for PIPE signals, and thus
            // we get an I/O error instead. Traditional C Unix applications
            // quit by getting a PIPE signal that they don't handle, and thus
            // the unhandled signal causes the process to unceremoniously
            // terminate.
            for cause in err.chain() {
                if let Some(ioerr) = cause.downcast_ref::<std::io::Error>() {
                    if ioerr.kind() == std::io::ErrorKind::BrokenPipe {
                        return ExitCode::from(0);
                    }
                }
            }
            eprintln_locked!("{:#}", err);
            ExitCode::from(2)
        }
    }
}

/// The main entry point for ripgrep.
///
/// The given parse result determines ripgrep's behavior. The parse
/// result should be the result of parsing CLI arguments in a low level
/// representation, and then followed by an attempt to convert them into a
/// higher level representation. The higher level representation has some nicer
/// abstractions, for example, instead of representing the `-g/--glob` flag
/// as a `Vec<String>` (as in the low level representation), the globs are
/// converted into a single matcher.
fn run(result: crate::flags::ParseResult<HiArgs>) -> anyhow::Result<ExitCode> {
    use crate::flags::{Mode, ParseResult};

    let args = match result {
        ParseResult::Err(err) => return Err(err),
        ParseResult::Special(mode) => return special(mode),
        ParseResult::Ok(args) => args,
    };
    let matched = if args.analyze() && args.watch() {
        return tokio::runtime::Runtime::new()?.block_on(analyze_and_watch(&args));
    } else if args.watch() {
        return tokio::runtime::Runtime::new()?.block_on(watch(&args));
    } else if args.tree() || args.analyze() || args.diff() || args.diagnostics() || args.syntax() {
        // Unified tree backbone for all analysis modes
        return tokio::runtime::Runtime::new()?.block_on(unified_tree_mode(&args));
    } else {
        match args.mode() {
            Mode::Search(_) if !args.matches_possible() => false,
            Mode::Search(mode) if args.threads() == 1 => search(&args, mode)?,
            Mode::Search(mode) => search_parallel(&args, mode)?,
            Mode::Files if args.threads() == 1 => files(&args)?,
            Mode::Files => files_parallel(&args)?,
            Mode::Types => return types(&args),
            Mode::Generate(mode) => return generate(mode),
        }
    };
    Ok(if matched && (args.quiet() || !messages::errored()) {
        ExitCode::from(0)
    } else if messages::errored() {
        ExitCode::from(2)
    } else {
        ExitCode::from(1)
    })
}

/// The top-level entry point for single-threaded search.
///
/// This recursively steps through the file list (current directory by default)
/// and searches each file sequentially.
fn search(args: &HiArgs, mode: SearchMode) -> anyhow::Result<bool> {
    let started_at = std::time::Instant::now();
    let haystack_builder = args.haystack_builder();
    let unsorted = args
        .walk_builder()?
        .build()
        .filter_map(|result| haystack_builder.build_from_result(result));
    let haystacks = args.sort(unsorted);

    let mut matched = false;
    let mut searched = false;
    let mut stats = args.stats();
    let mut searcher = args.search_worker(
        args.matcher()?,
        args.searcher()?,
        args.printer(mode, args.stdout()),
    )?;
    for haystack in haystacks {
        searched = true;
        let search_result = match searcher.search(&haystack) {
            Ok(search_result) => search_result,
            // A broken pipe means graceful termination.
            Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => break,
            Err(err) => {
                err_message!("{}: {}", haystack.path().display(), err);
                continue;
            }
        };
        matched = matched || search_result.has_match();
        if let Some(ref mut stats) = stats {
            *stats += search_result.stats().unwrap();
        }
        if matched && args.quit_after_match() {
            break;
        }
    }
    if args.has_implicit_path() && !searched {
        eprint_nothing_searched();
    }
    if let Some(ref stats) = stats {
        let wtr = searcher.printer().get_mut();
        let _ = print_stats(mode, stats, started_at, wtr);
    }
    Ok(matched)
}

/// The top-level entry point for multi-threaded search.
///
/// The parallelism is itself achieved by the recursive directory traversal.
/// All we need to do is feed it a worker for performing a search on each file.
///
/// Requesting a sorted output from ripgrep (such as with `--sort path`) will
/// automatically disable parallelism and hence sorting is not handled here.
fn search_parallel(args: &HiArgs, mode: SearchMode) -> anyhow::Result<bool> {
    use std::sync::atomic::{AtomicBool, Ordering};

    let started_at = std::time::Instant::now();
    let haystack_builder = args.haystack_builder();
    let bufwtr = args.buffer_writer();
    let stats = args.stats().map(std::sync::Mutex::new);
    let matched = AtomicBool::new(false);
    let searched = AtomicBool::new(false);

    let mut searcher = args.search_worker(
        args.matcher()?,
        args.searcher()?,
        args.printer(mode, bufwtr.buffer()),
    )?;
    args.walk_builder()?.build_parallel().run(|| {
        let bufwtr = &bufwtr;
        let stats = &stats;
        let matched = &matched;
        let searched = &searched;
        let haystack_builder = &haystack_builder;
        let mut searcher = searcher.clone();

        Box::new(move |result| {
            let haystack = match haystack_builder.build_from_result(result) {
                Some(haystack) => haystack,
                None => return WalkState::Continue,
            };
            searched.store(true, Ordering::SeqCst);
            searcher.printer().get_mut().clear();
            let search_result = match searcher.search(&haystack) {
                Ok(search_result) => search_result,
                Err(err) => {
                    err_message!("{}: {}", haystack.path().display(), err);
                    return WalkState::Continue;
                }
            };
            if search_result.has_match() {
                matched.store(true, Ordering::SeqCst);
            }
            if let Some(ref locked_stats) = *stats {
                let mut stats = locked_stats.lock().unwrap();
                *stats += search_result.stats().unwrap();
            }
            if let Err(err) = bufwtr.print(searcher.printer().get_mut()) {
                // A broken pipe means graceful termination.
                if err.kind() == std::io::ErrorKind::BrokenPipe {
                    return WalkState::Quit;
                }
                // Otherwise, we continue on our merry way.
                err_message!("{}: {}", haystack.path().display(), err);
            }
            if matched.load(Ordering::SeqCst) && args.quit_after_match() {
                WalkState::Quit
            } else {
                WalkState::Continue
            }
        })
    });
    if args.has_implicit_path() && !searched.load(Ordering::SeqCst) {
        eprint_nothing_searched();
    }
    if let Some(ref locked_stats) = stats {
        let stats = locked_stats.lock().unwrap();
        let mut wtr = searcher.printer().get_mut();
        let _ = print_stats(mode, &stats, started_at, &mut wtr);
        let _ = bufwtr.print(&mut wtr);
    }
    Ok(matched.load(Ordering::SeqCst))
}

/// The top-level entry point for file listing without searching.
///
/// This recursively steps through the file list (current directory by default)
/// and prints each path sequentially using a single thread.
fn files(args: &HiArgs) -> anyhow::Result<bool> {
    let haystack_builder = args.haystack_builder();
    let unsorted = args
        .walk_builder()?
        .build()
        .filter_map(|result| haystack_builder.build_from_result(result));
    let haystacks = args.sort(unsorted);

    let mut matched = false;
    let mut path_printer = args.path_printer_builder().build(args.stdout());
    for haystack in haystacks {
        matched = true;
        if args.quit_after_match() {
            break;
        }
        if let Err(err) = path_printer.write(haystack.path()) {
            // A broken pipe means graceful termination.
            if err.kind() == std::io::ErrorKind::BrokenPipe {
                break;
            }
            // Otherwise, we have some other error that's preventing us from
            // writing to stdout, so we should bubble it up.
            return Err(err.into());
        }
    }
    Ok(matched)
}

/// The top-level entry point for multi-threaded file listing without
/// searching.
///
/// This recursively steps through the file list (current directory by default)
/// and prints each path sequentially using multiple threads.
///
/// Requesting a sorted output from ripgrep (such as with `--sort path`) will
/// automatically disable parallelism and hence sorting is not handled here.
fn files_parallel(args: &HiArgs) -> anyhow::Result<bool> {
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread,
    };

    let haystack_builder = args.haystack_builder();
    let mut path_printer = args.path_printer_builder().build(args.stdout());
    let matched = AtomicBool::new(false);
    let (tx, rx) = mpsc::channel::<crate::haystack::Haystack>();

    // We spawn a single printing thread to make sure we don't tear writes.
    // We use a channel here under the presumption that it's probably faster
    // than using a mutex in the worker threads below, but this has never been
    // seriously litigated.
    let print_thread = thread::spawn(move || -> std::io::Result<()> {
        for haystack in rx.iter() {
            path_printer.write(haystack.path())?;
        }
        Ok(())
    });
    args.walk_builder()?.build_parallel().run(|| {
        let haystack_builder = &haystack_builder;
        let matched = &matched;
        let tx = tx.clone();

        Box::new(move |result| {
            let haystack = match haystack_builder.build_from_result(result) {
                Some(haystack) => haystack,
                None => return WalkState::Continue,
            };
            matched.store(true, Ordering::SeqCst);
            if args.quit_after_match() {
                WalkState::Quit
            } else {
                match tx.send(haystack) {
                    Ok(_) => WalkState::Continue,
                    Err(_) => WalkState::Quit,
                }
            }
        })
    });
    drop(tx);
    if let Err(err) = print_thread.join().unwrap() {
        // A broken pipe means graceful termination, so fall through.
        // Otherwise, something bad happened while writing to stdout, so bubble
        // it up.
        if err.kind() != std::io::ErrorKind::BrokenPipe {
            return Err(err.into());
        }
    }
    Ok(matched.load(Ordering::SeqCst))
}

/// The top-level entry point for `--type-list`.
fn types(args: &HiArgs) -> anyhow::Result<ExitCode> {
    let mut count = 0;
    let mut stdout = args.stdout();
    for def in args.types().definitions() {
        count += 1;
        stdout.write_all(def.name().as_bytes())?;
        stdout.write_all(b": ")?;

        let mut first = true;
        for glob in def.globs() {
            if !first {
                stdout.write_all(b", ")?;
            }
            stdout.write_all(glob.as_bytes())?;
            first = false;
        }
        stdout.write_all(b"\n")?;
    }
    Ok(ExitCode::from(if count == 0 { 1 } else { 0 }))
}

/// Implements ripgrep's "generate" modes.
///
/// These modes correspond to generating some kind of ancillary data related
/// to ripgrep. At present, this includes ripgrep's man page (in roff format)
/// and supported shell completions.
fn generate(mode: crate::flags::GenerateMode) -> anyhow::Result<ExitCode> {
    use crate::flags::GenerateMode;

    let output = match mode {
        GenerateMode::Man => flags::generate_man_page(),
        GenerateMode::CompleteBash => flags::generate_complete_bash(),
        GenerateMode::CompleteZsh => flags::generate_complete_zsh(),
        GenerateMode::CompleteFish => flags::generate_complete_fish(),
        GenerateMode::CompletePowerShell => {
            flags::generate_complete_powershell()
        }
    };
    writeln!(std::io::stdout(), "{}", output.trim_end())?;
    Ok(ExitCode::from(0))
}

/// Implements ripgrep's "special" modes.
///
/// A special mode is one that generally short-circuits most (not all) of
/// ripgrep's initialization logic and skips right to this routine. The
/// special modes essentially consist of printing help and version output. The
/// idea behind the short circuiting is to ensure there is as little as possible
/// (within reason) that would prevent ripgrep from emitting help output.
///
/// For example, part of the initialization logic that is skipped (among
/// other things) is accessing the current working directory. If that fails,
/// ripgrep emits an error. We don't want to emit an error if it fails and
/// the user requested version or help information.
fn special(mode: crate::flags::SpecialMode) -> anyhow::Result<ExitCode> {
    use crate::flags::SpecialMode;

    let mut exit = ExitCode::from(0);
    let output = match mode {
        SpecialMode::HelpShort => flags::generate_help_short(),
        SpecialMode::HelpLong => flags::generate_help_long(),
        SpecialMode::VersionShort => flags::generate_version_short(),
        SpecialMode::VersionLong => flags::generate_version_long(),
        // --pcre2-version is a little special because it emits an error
        // exit code if this build of ripgrep doesn't support PCRE2.
        SpecialMode::VersionPCRE2 => {
            let (output, available) = flags::generate_version_pcre2();
            if !available {
                exit = ExitCode::from(1);
            }
            output
        }
        // Config management special modes
        SpecialMode::ConfigStatus => {
            match flags::ConfigManager::show_config_status() {
                Ok(()) => return Ok(ExitCode::from(0)),
                Err(e) => {
                    writeln!(std::io::stderr(), "Error: {}", e)?;
                    return Ok(ExitCode::from(1));
                }
            }
        }
        SpecialMode::InitGlobalConfig => {
            match flags::ConfigManager::init_global_config(false) {
                Ok(path) => {
                    writeln!(std::io::stdout(), "Global config created at: {}", path.display())?;
                    return Ok(ExitCode::from(0));
                }
                Err(e) => {
                    writeln!(std::io::stderr(), "Error: {}", e)?;
                    return Ok(ExitCode::from(1));
                }
            }
        }
        SpecialMode::InitLocalConfig => {
            match flags::ConfigManager::init_local_config(false) {
                Ok(path) => {
                    writeln!(std::io::stdout(), "Local config created at: {}", path.display())?;
                    return Ok(ExitCode::from(0));
                }
                Err(e) => {
                    writeln!(std::io::stderr(), "Error: {}", e)?;
                    return Ok(ExitCode::from(1));
                }
            }
        }
        SpecialMode::OpenGlobalConfig => {
            match flags::ConfigManager::open_global_config() {
                Ok(()) => return Ok(ExitCode::from(0)),
                Err(e) => {
                    writeln!(std::io::stderr(), "Error: {}", e)?;
                    return Ok(ExitCode::from(1));
                }
            }
        }
        SpecialMode::OpenLocalConfig => {
            match flags::ConfigManager::open_local_config() {
                Ok(()) => return Ok(ExitCode::from(0)),
                Err(e) => {
                    writeln!(std::io::stderr(), "Error: {}", e)?;
                    return Ok(ExitCode::from(1));
                }
            }
        }
    };
    writeln!(std::io::stdout(), "{}", output.trim_end())?;
    Ok(exit)
}

/// Prints a heuristic error messages when nothing is searched.
///
/// This can happen if an applicable ignore file has one or more rules that
/// are too broad and cause ripgrep to ignore everything.
///
/// We only show this error message when the user does *not* provide an
/// explicit path to search. This is because the message can otherwise be
/// noisy, e.g., when it is intended that there is nothing to search.
fn eprint_nothing_searched() {
    err_message!(
        "No files were searched, which means ripgrep probably \
         applied a filter you didn't expect.\n\
         Running with --debug will show why files are being skipped."
    );
}

/// Prints the statistics given to the writer given.
///
/// The search mode given determines whether the stats should be printed in
/// a plain text format or in a JSON format.
///
/// The `started` time should be the time at which ripgrep started working.
///
/// If an error occurs while writing, then writing stops and the error is
/// returned. Note that callers should probably ignore this errror, since
/// whether stats fail to print or not generally shouldn't cause ripgrep to
/// enter into an "error" state. And usually the only way for this to fail is
/// if writing to stdout itself fails.
fn print_stats<W: Write>(
    mode: SearchMode,
    stats: &grep::printer::Stats,
    started: std::time::Instant,
    mut wtr: W,
) -> std::io::Result<()> {
    let elapsed = std::time::Instant::now().duration_since(started);
    if matches!(mode, SearchMode::JSON) {
        // We specifically match the format laid out by the JSON printer in
        // the grep-printer crate. We simply "extend" it with the 'summary'
        // message type.
        serde_json::to_writer(
            &mut wtr,
            &serde_json::json!({
                "type": "summary",
                "data": {
                    "stats": stats,
                    "elapsed_total": {
                        "secs": elapsed.as_secs(),
                        "nanos": elapsed.subsec_nanos(),
                        "human": format!("{:0.6}s", elapsed.as_secs_f64()),
                    },
                }
            }),
        )?;
        write!(wtr, "\n")
    } else {
        write!(
            wtr,
            "
{matches} matches
{lines} matched lines
{searches_with_match} files contained matches
{searches} files searched
{bytes_printed} bytes printed
{bytes_searched} bytes searched
{search_time:0.6} seconds spent searching
{process_time:0.6} seconds
",
            matches = stats.matches(),
            lines = stats.matched_lines(),
            searches_with_match = stats.searches_with_match(),
            searches = stats.searches(),
            bytes_printed = stats.bytes_printed(),
            bytes_searched = stats.bytes_searched(),
            search_time = stats.elapsed().as_secs_f64(),
            process_time = elapsed.as_secs_f64(),
        )
    }
}

/// Entry point for analyze mode.
///
/// This function performs a one-time analysis of the current directory
/// and displays code metrics and Git status.
async fn analyze(args: &HiArgs) -> anyhow::Result<ExitCode> {
    use crate::diagnostics::{MetricsCalculator, GitAnalyzer};
    
    println!("Outgrep Code Intelligence Analysis");
    println!("=====================================");
    println!();
    
    // Use current directory for analysis
    let current_dir = std::path::Path::new(".");
    
    println!("Analyzing directory: {}", current_dir.display());
    println!();
    
    // Initialize Git analyzer to get changed files
    let git_analyzer = GitAnalyzer::new(current_dir);
    let git_status = git_analyzer.get_status_for_cwd().unwrap_or_default();
    let git_diagnostics = git_analyzer.get_diagnostics().ok();
    
    // Walk through files and calculate metrics
    let mut total_files = 0;
    let mut total_loc = 0;
    let mut total_comments = 0;
    let mut total_functions = 0;
    let mut total_complexity = 0;
    
    let walker = ignore::WalkBuilder::new(current_dir)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .parents(true)
        .build();
    
    for result in walker {
        let entry = match result {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("Warning: {}", err);
                continue;
            }
        };
        
        // Skip directories
        if entry.file_type().map_or(false, |ft| ft.is_dir()) {
            continue;
        }
        
        let path = entry.path();
        
        // Skip common lock files and generated files
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            match file_name {
                "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | 
                "composer.lock" | "Gemfile.lock" | "poetry.lock" | "Pipfile.lock" => {
                    continue;
                }
                _ => {}
            }
        }
        
        // Only analyze source files
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext {
                "rs" | "js" | "jsx" | "ts" | "tsx" | "py" | "java" | "go" | 
                "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "php" | "rb" | 
                "cs" | "swift" => {
                    // Calculate metrics for this file
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Ok(metrics) = MetricsCalculator::calculate_metrics(path, &content) {
                            total_files += 1;
                            total_loc += metrics.lines_of_code;
                            total_comments += metrics.comment_lines;
                            total_functions += metrics.function_count as u64;
                            total_complexity += metrics.cyclomatic_complexity as u64;
                            
                            let relative_path = path.strip_prefix(current_dir).unwrap_or(path);
                            let status_icon = if let Some(git_status) = git_status.get(relative_path) {
                                match git_status {
                                    crate::diagnostics::GitFileStatus::Modified => "M",
                                    crate::diagnostics::GitFileStatus::Staged => "S",
                                    crate::diagnostics::GitFileStatus::Untracked => "?",
                                    crate::diagnostics::GitFileStatus::Conflicted => "!",
                                }
                            } else {
                                ""
                            };
                            
                            println!("{} {}: {}", 
                                status_icon,
                                relative_path.display(),
                                MetricsCalculator::metrics_summary(&metrics)
                            );
                            
                            // Show inline diff if file has changes and diff flag is enabled
                            if args.diff() && matches!(git_status.get(relative_path), Some(crate::diagnostics::GitFileStatus::Modified) | Some(crate::diagnostics::GitFileStatus::Staged)) {
                                match git_analyzer.get_semantic_diff(path) {
                                    Ok(diff) => {
                                        if !diff.trim().is_empty() {
                                            println!("    ┌─ Diff:");
                                            for line in diff.lines() {
                                                println!("    │ {}", line);
                                            }
                                            println!("    └─");
                                        }
                                    }
                                    Err(e) => {
                                        println!("    ┌─ Diff Error: {}", e);
                                        println!("    └─");
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    
    println!();
    println!("Summary Statistics:");
    println!("  Files analyzed: {}", total_files);
    println!("  Total lines of code: {}", total_loc);
    println!("  Total comment lines: {}", total_comments);
    println!("  Total functions: {}", total_functions);
    println!("  Average complexity: {:.1}", 
        if total_functions > 0 { total_complexity as f64 / total_functions as f64 } else { 0.0 }
    );
    
    // Add Git status information at the bottom (summary section)
    if let Some(git_diagnostics) = git_diagnostics {
        println!();
        println!("Git Status: {}", git_analyzer.diagnostics_summary(&git_diagnostics));
    }
    
    // Show diffs for changed files if diff flag is enabled
    if args.diff() && !git_status.is_empty() {
        println!();
        println!("Semantic Diffs for Changed Files:");
        println!("{}", "═".repeat(60));
        
        for (relative_path, status) in &git_status {
            // Skip lock files
            if let Some(file_name) = relative_path.file_name().and_then(|n| n.to_str()) {
                match file_name {
                    "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | 
                    "composer.lock" | "Gemfile.lock" | "poetry.lock" | "Pipfile.lock" => {
                        continue;
                    }
                    _ => {}
                }
            }
            
            match status {
                crate::diagnostics::GitFileStatus::Modified | 
                crate::diagnostics::GitFileStatus::Staged => {
                    let full_path = current_dir.join(relative_path);
                    if let Err(e) = show_semantic_diff(&full_path, &git_analyzer) {
                        eprintln!("Warning: Could not show diff for {}: {}", relative_path.display(), e);
                    }
                }
                _ => {} // Skip untracked and conflicted files
            }
        }
    }
    
    Ok(ExitCode::from(0))
}

/// Entry point for standalone diff mode.
///
/// This function shows diffs for all changed files in the current directory.
async fn diff_only(args: &HiArgs) -> anyhow::Result<ExitCode> {
    use crate::diagnostics::GitAnalyzer;
    
    println!("Outgrep Git Diff Analysis");
    println!("============================");
    println!();
    
    // Use current directory for analysis
    let current_dir = std::path::Path::new(".");
    
    // Initialize Git analyzer to get changed files
    let git_analyzer = GitAnalyzer::new(current_dir);
    let git_status = git_analyzer.get_status_for_cwd().unwrap_or_default();
    let git_diagnostics = git_analyzer.get_diagnostics().ok();
    
    if git_status.is_empty() {
        println!("No changes detected in current directory.");
        return Ok(ExitCode::from(0));
    }
    
    // Display git status summary
    if let Some(git_diagnostics) = git_diagnostics {
        println!("Git Status: {}", git_analyzer.diagnostics_summary(&git_diagnostics));
        println!();
    }
    
    // Show diffs for all changed files (excluding lock files)
    let mut diff_count = 0;
    for (relative_path, status) in &git_status {
        // Skip lock files
        if let Some(file_name) = relative_path.file_name().and_then(|n| n.to_str()) {
            match file_name {
                "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | 
                "composer.lock" | "Gemfile.lock" | "poetry.lock" | "Pipfile.lock" => {
                    continue;
                }
                _ => {}
            }
        }
        
        match status {
            crate::diagnostics::GitFileStatus::Modified | 
            crate::diagnostics::GitFileStatus::Staged => {
                let status_icon = match status {
                    crate::diagnostics::GitFileStatus::Modified => "M",
                    crate::diagnostics::GitFileStatus::Staged => "S",
                    _ => "",
                };
                
                println!("{} {}", status_icon, relative_path.display());
                
                // Convert relative path to absolute path for diff
                let absolute_path = std::env::current_dir()?.join(relative_path);
                
                match git_analyzer.get_semantic_diff(&absolute_path) {
                    Ok(diff) => {
                        if !diff.trim().is_empty() {
                            println!("┌─ Diff:");
                            for line in diff.lines() {
                                println!("│ {}", line);
                            }
                            println!("└─");
                            diff_count += 1;
                        } else {
                            println!("└─ No changes or whitespace only");
                        }
                    }
                    Err(e) => {
                        println!("└─ Diff Error: {}", e);
                    }
                }
                println!();
            }
            _ => {} // Skip untracked and conflicted files for diff
        }
    }
    
    if diff_count == 0 {
        println!("No file diffs to display (files may be untracked or have no changes).");
    } else {
        println!("Displayed {} file diff(s)", diff_count);
    }
    
    Ok(ExitCode::from(0))
}

/// Entry point for standalone tree mode.
async fn tree_only(args: &HiArgs) -> anyhow::Result<ExitCode> {
    use crate::diagnostics::{GitAnalyzer, TreeBuilder, TreeDisplay, TreeDisplayOptions};
    
    println!("Outgrep Tree View");
    println!("===================");
    println!();
    
    // For tree mode, use current directory by default
    let root_path_buf = std::path::PathBuf::from(".");
    
    // Initialize Git analyzer for git status (optional)
    let git_analyzer = GitAnalyzer::new(&root_path_buf);
    let git_status = git_analyzer.get_status_for_cwd().unwrap_or_default();
    
    // Display git status summary if available
    if !git_status.is_empty() {
        let git_diagnostics = git_analyzer.get_diagnostics().ok();
        if let Some(git_diagnostics) = git_diagnostics {
            println!("Git Status: {}", git_analyzer.diagnostics_summary(&git_diagnostics));
            println!();
        }
    }
    
    // Build and display tree
    let options = TreeDisplayOptions {
        show_metrics: false,
        show_diffs: false,
        show_analysis: false,
        show_diagnostics: args.diagnostics(),
        show_syntax: args.syntax(),
        truncate_diffs: args.truncate_diffs(),
        output_json: args.json_output(),
        git_status: git_status.clone(),
    };
    
    let tree_builder = TreeBuilder::with_options(&root_path_buf, options.clone());
    match tree_builder.build_tree(&root_path_buf) {
        Ok(tree) => {
            
            if args.json_output() {
                TreeDisplay::output_json(&tree, &options);
            } else {
                TreeDisplay::display_tree_with_options(&tree, &options);
            }
        }
        Err(e) => {
            eprintln!("Error building tree: {}", e);
            return Ok(ExitCode::from(1));
        }
    }
    
    Ok(ExitCode::from(0))
}

/// Entry point for tree mode with diff integration.
async fn tree_with_diff(args: &HiArgs) -> anyhow::Result<ExitCode> {
    use crate::diagnostics::{GitAnalyzer, TreeBuilder, TreeDisplay, TreeDisplayOptions};
    
    println!("Outgrep Git Diff Analysis");
    println!("============================");
    println!();
    
    // Extract path from command line arguments
    let root_path_buf = std::env::args_os()
        .last()
        .and_then(|last_arg| {
            let path_str = last_arg.to_string_lossy();
            if path_str.starts_with('-') || path_str == "og" {
                None
            } else {
                Some(std::path::PathBuf::from(path_str.as_ref()))
            }
        })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    
    // Initialize Git analyzer and tree builder
    let git_analyzer = GitAnalyzer::new(&root_path_buf);
    let git_status = git_analyzer.get_status_for_cwd().unwrap_or_default();
    let git_diagnostics = git_analyzer.get_diagnostics().ok();
    
    // Display git status summary
    if let Some(git_diagnostics) = git_diagnostics {
        println!("Git Status: {}", git_analyzer.diagnostics_summary(&git_diagnostics));
        println!();
    }
    
    println!("Directory Tree");
    println!("=================");
    println!();
    
    // Build and display tree with diff information
    let options = TreeDisplayOptions {
        show_metrics: false,
        show_diffs: true,
        show_analysis: false,
        show_diagnostics: args.diagnostics(),
        show_syntax: args.syntax(),
        truncate_diffs: args.truncate_diffs(),
        output_json: args.json_output(),
        git_status: git_status.clone(),
    };
    
    let tree_builder = TreeBuilder::with_options(&root_path_buf, options.clone());
    match tree_builder.build_tree(&root_path_buf) {
        Ok(tree) => {
            
            TreeDisplay::display_tree_with_options(&tree, &options);
        }
        Err(e) => {
            eprintln!("Error building tree: {}", e);
            return Ok(ExitCode::from(1));
        }
    }
    
    Ok(ExitCode::from(0))
}

/// Show semantic diff for a file using the similar crate
fn show_semantic_diff(path: &std::path::Path, git_analyzer: &crate::diagnostics::GitAnalyzer) -> Result<(), Box<dyn std::error::Error>> {
    use similar::{ChangeTag, TextDiff};
    
    // Get the current content
    let current_content = std::fs::read_to_string(path)?;
    
    // Get the Git HEAD content for comparison
    let head_content = git_analyzer.get_file_at_head(path)?;
    
    // Create a diff
    let diff = TextDiff::from_lines(&head_content, &current_content);
    
    println!("\n{}", path.display());
    println!("{}", "─".repeat(50));
    
    let mut has_changes = false;
    for change in diff.iter_all_changes() {
        has_changes = true;
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        print!("{}{}", sign, change);
    }
    
    if !has_changes {
        println!("No changes detected");
    }
    
    Ok(())
}

/// Entry point for watch mode.
///
/// This function starts file watching for real-time monitoring of file changes.
async fn watch(args: &HiArgs) -> anyhow::Result<ExitCode> {
    use crate::diagnostics::{FileWatcher, MetricsCalculator};
    use std::io::Write;
    use std::time::Duration;
    
    let current_dir = std::path::Path::new(".");
    
    println!("Outgrep File Watcher");
    println!("========================");
    println!("Watching for changes in: {}", current_dir.display());
    println!("Press Ctrl+C to exit...");
    println!();
    
    let mut watcher = FileWatcher::new()?;
    watcher.watch(current_dir)?;
    
    // Watch for file changes
    loop {
        if let Some(event) = watcher.next_event_timeout(Duration::from_secs(1)).await {
            match event {
                crate::diagnostics::FileChangeEvent::Created(path) => {
                    println!("File created: {}", path.display());
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(metrics) = MetricsCalculator::calculate_metrics(&path, &content) {
                            println!("   {}", MetricsCalculator::metrics_summary(&metrics));
                        }
                    }
                }
                crate::diagnostics::FileChangeEvent::Modified(path) => {
                    println!("File modified: {}", path.display());
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(metrics) = MetricsCalculator::calculate_metrics(&path, &content) {
                            println!("   {}", MetricsCalculator::metrics_summary(&metrics));
                        }
                    }
                }
                crate::diagnostics::FileChangeEvent::Deleted(path) => {
                    println!("File deleted: {}", path.display());
                }
                crate::diagnostics::FileChangeEvent::Renamed { from, to } => {
                    println!("File renamed: {} -> {}", from.display(), to.display());
                }
            }
            std::io::stdout().flush().unwrap();
        }
    }
}

/// Entry point for combined analyze and watch mode.
///
/// This function performs initial analysis and then starts file watching.
async fn analyze_and_watch(args: &HiArgs) -> anyhow::Result<ExitCode> {
    use crate::diagnostics::{FileWatcher, MetricsCalculator, GitAnalyzer};
    use std::io::Write;
    use std::time::Duration;
    
    // First, perform the analysis
    let current_dir = std::path::Path::new(".");
    
    println!("Outgrep Code Intelligence Analysis & Watch");
    println!("==============================================");
    println!();
    
    println!("Analyzing directory: {}", current_dir.display());
    
    // Initialize Git analyzer and get status
    let git_analyzer = GitAnalyzer::new(current_dir);
    let git_status = git_analyzer.get_status_for_cwd().unwrap_or_default();
    let git_diagnostics = git_analyzer.get_diagnostics().ok();
    
    // Walk through files and calculate metrics
    let mut total_files = 0;
    let mut total_loc = 0;
    let mut total_comments = 0;
    let mut total_functions = 0;
    let mut total_complexity = 0;
    
    let walker = ignore::WalkBuilder::new(current_dir)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .parents(true)
        .build();
    
    for result in walker {
        let entry = match result {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("Warning: {}", err);
                continue;
            }
        };
        
        // Skip directories
        if entry.file_type().map_or(false, |ft| ft.is_dir()) {
            continue;
        }
        
        let path = entry.path();
        
        // Skip common lock files and generated files
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            match file_name {
                "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | 
                "composer.lock" | "Gemfile.lock" | "poetry.lock" | "Pipfile.lock" => {
                    continue;
                }
                _ => {}
            }
        }
        
        // Only analyze source files
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext {
                "rs" | "js" | "jsx" | "ts" | "tsx" | "py" | "java" | "go" | 
                "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "php" | "rb" | 
                "cs" | "swift" => {
                    // Calculate metrics for this file
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Ok(metrics) = MetricsCalculator::calculate_metrics(path, &content) {
                            total_files += 1;
                            total_loc += metrics.lines_of_code;
                            total_comments += metrics.comment_lines;
                            total_functions += metrics.function_count as u64;
                            total_complexity += metrics.cyclomatic_complexity as u64;
                            
                            let relative_path = path.strip_prefix(current_dir).unwrap_or(path);
                            let status_icon = if let Some(git_status) = git_status.get(relative_path) {
                                match git_status {
                                    crate::diagnostics::GitFileStatus::Modified => "M",
                                    crate::diagnostics::GitFileStatus::Staged => "S",
                                    crate::diagnostics::GitFileStatus::Untracked => "?",
                                    crate::diagnostics::GitFileStatus::Conflicted => "!",
                                }
                            } else {
                                ""
                            };
                            
                            println!("{} {}: {}", 
                                status_icon,
                                relative_path.display(),
                                MetricsCalculator::metrics_summary(&metrics)
                            );
                            
                            // Show inline diff if file has changes and diff flag is enabled
                            if args.diff() && matches!(git_status.get(relative_path), Some(crate::diagnostics::GitFileStatus::Modified) | Some(crate::diagnostics::GitFileStatus::Staged)) {
                                match git_analyzer.get_semantic_diff(path) {
                                    Ok(diff) => {
                                        if !diff.trim().is_empty() {
                                            println!("    ┌─ Diff:");
                                            for line in diff.lines() {
                                                println!("    │ {}", line);
                                            }
                                            println!("    └─");
                                        }
                                    }
                                    Err(e) => {
                                        println!("    ┌─ Diff Error: {}", e);
                                        println!("    └─");
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    
    println!();
    println!("Summary Statistics:");
    println!("  Files analyzed: {}", total_files);
    println!("  Total lines of code: {}", total_loc);
    println!("  Total comment lines: {}", total_comments);
    println!("  Total functions: {}", total_functions);
    println!("  Average complexity: {:.1}", 
        if total_functions > 0 { total_complexity as f64 / total_functions as f64 } else { 0.0 }
    );
    
    // Add Git status information at the bottom (summary section)
    if let Some(git_diagnostics) = git_diagnostics {
        println!();
        println!("Git Status: {}", git_analyzer.diagnostics_summary(&git_diagnostics));
    }
    println!();
    
    // Now start file watching
    println!("Starting file watcher (press Ctrl+C to exit)...");
    println!("Watching for changes in: {}", current_dir.display());
    println!();
    
    let mut watcher = FileWatcher::new()?;
    watcher.watch(current_dir)?;
    
    // Watch for file changes
    loop {
        if let Some(event) = watcher.next_event_timeout(Duration::from_secs(1)).await {
            match event {
                crate::diagnostics::FileChangeEvent::Created(path) => {
                    println!("File created: {}", path.display());
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(metrics) = MetricsCalculator::calculate_metrics(&path, &content) {
                            println!("   {}", MetricsCalculator::metrics_summary(&metrics));
                        }
                    }
                }
                crate::diagnostics::FileChangeEvent::Modified(path) => {
                    println!("File modified: {}", path.display());
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(metrics) = MetricsCalculator::calculate_metrics(&path, &content) {
                            println!("   {}", MetricsCalculator::metrics_summary(&metrics));
                        }
                    }
                }
                crate::diagnostics::FileChangeEvent::Deleted(path) => {
                    println!("File deleted: {}", path.display());
                }
                crate::diagnostics::FileChangeEvent::Renamed { from, to } => {
                    println!("File renamed: {} -> {}", from.display(), to.display());
                }
            }
            std::io::stdout().flush().unwrap();
        }
    }
}

/// Entry point for unified tree mode that integrates all analysis types
///
/// This function serves as the backbone for integrating tree, diff, analyze, and diagnostics
/// into a single coherent view when any of these flags are enabled.
async fn unified_tree_mode(args: &HiArgs) -> anyhow::Result<ExitCode> {
    use crate::diagnostics::{GitAnalyzer, TreeBuilder, TreeDisplay, TreeDisplayOptions};
    
    // Use current directory for analysis
    let root_path_buf = std::path::PathBuf::from(".");
    
    // Initialize Git analyzer and tree builder
    let git_analyzer = GitAnalyzer::new(&root_path_buf);
    let git_status = git_analyzer.get_status_for_cwd().unwrap_or_default();
    let git_diagnostics = git_analyzer.get_diagnostics().ok();
    
    // Only show headers and status when NOT in JSON output mode
    if !args.json_output() {
        // Determine header based on active flags - build dynamically
        let mut features = Vec::new();
        if args.tree() { features.push("Tree"); }
        if args.diff() { features.push("Diff"); }
        if args.analyze() { features.push("Analysis"); }
        if args.diagnostics() { features.push("Diagnostics"); }
        if args.syntax() { features.push("Syntax"); }
        
        let header = if features.is_empty() {
            "Outgrep Analysis".to_string()
        } else if features.len() == 1 {
            match features[0] {
                "Tree" => "Outgrep Tree View".to_string(),
                "Diff" => "Outgrep Git Diff Analysis".to_string(),
                "Analysis" => "Outgrep Code Intelligence Analysis".to_string(),
                "Diagnostics" => "Outgrep Compiler Diagnostics".to_string(),
                "Syntax" => "Outgrep Syntax Analysis".to_string(),
                _ => "Outgrep Analysis".to_string()
            }
        } else {
            format!("Outgrep Unified Analysis ({})", features.join(" + "))
        };
        
        println!("{}", header);
        println!("{}", "=".repeat(header.len()));
        println!();
        
        // Display git status summary if available and relevant
        if !git_status.is_empty() && (args.diff() || args.tree()) {
            if let Some(git_diagnostics) = git_diagnostics {
                println!("Git Status: {}", git_analyzer.diagnostics_summary(&git_diagnostics));
                println!();
            }
        }
    }
    
    // Handle tree mode or file-centric mode
    if args.tree() || args.syntax() {
        // Tree backbone mode - integrate everything into tree structure
        if !args.json_output() && args.tree() {
            println!("Directory Tree");
            println!("=================");
            println!();
        }
        
        // Create TreeDisplayOptions based on individual flags
        let options = TreeDisplayOptions {
            show_metrics: args.analyze(),
            show_diffs: args.diff(),
            show_analysis: args.analyze(),
            show_diagnostics: args.diagnostics(),
            show_syntax: args.syntax(),
            truncate_diffs: args.truncate_diffs(),
            output_json: args.json_output(),
            git_status: git_status.clone(),
        };
        
        let tree_builder = TreeBuilder::with_options(&root_path_buf, options.clone());
        match tree_builder.build_tree(&root_path_buf) {
            Ok(tree) => {
                
                if args.json_output() {
                    output_unified_json(&tree, &options, args, &git_status).await;
                } else {
                    TreeDisplay::display_tree_with_options(&tree, &options);
                }
            }
            Err(e) => {
                eprintln!("Error building tree: {}", e);
                return Ok(ExitCode::from(1));
            }
        }
    } else {
        // File-centric mode - show full paths with integrated analysis
        use crate::diagnostics::MetricsCalculator;
        
        // Walk through files and show file-centric information
        let walker = ignore::WalkBuilder::new(&root_path_buf)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .ignore(true)
            .parents(true)
            .build();
        
        let mut analyzed_files = 0;
        let mut total_files = 0;
        let mut total_loc = 0;
        let mut total_comments = 0;
        let mut total_functions = 0;
        let mut total_complexity = 0;
        
        for result in walker {
            let entry = match result {
                Ok(entry) => entry,
                Err(err) => {
                    eprintln!("Warning: {}", err);
                    continue;
                }
            };
            
            // Skip directories
            if entry.file_type().map_or(false, |ft| ft.is_dir()) {
                continue;
            }
            
            let path = entry.path();
            
            // Skip common lock files and generated files
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                match file_name {
                    "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | 
                    "composer.lock" | "Gemfile.lock" | "poetry.lock" | "Pipfile.lock" => {
                        continue;
                    }
                    _ => {}
                }
            }
            
            let relative_path = path.strip_prefix(&root_path_buf).unwrap_or(path);
            
            // Check if this file should be displayed
            let should_display = if args.diff() {
                // For diff mode, only show files with changes
                git_status.contains_key(relative_path)
            } else {
                // For other modes, show all source files or all files based on context
                if args.analyze() || args.diagnostics() {
                    // Show only source files for analysis/diagnostics
                    path.extension()
                        .and_then(|e| e.to_str())
                        .map(|ext| matches!(ext, 
                            "rs" | "js" | "jsx" | "ts" | "tsx" | "py" | "java" | "go" | 
                            "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "php" | "rb" | 
                            "cs" | "swift" | "kt" | "scala" | "clj" | "cljs" | "hs" | 
                            "elm" | "ex" | "exs" | "erl" | "lua" | "r" | "jl" | "dart"
                        ))
                        .unwrap_or(false)
                } else {
                    true
                }
            };
            
            if !should_display {
                continue;
            }
            
            // Get git status for this file
            let file_git_status = git_status.get(relative_path);
            let status_icon = if let Some(status) = file_git_status {
                match status {
                    crate::diagnostics::GitFileStatus::Modified => "M",
                    crate::diagnostics::GitFileStatus::Staged => "S",
                    crate::diagnostics::GitFileStatus::Untracked => "?",
                    crate::diagnostics::GitFileStatus::Conflicted => "!",
                }
            } else {
                ""
            };
            
            // Display file with full path
            print!("{} {}", status_icon, relative_path.display());
            
            // Add analysis information if requested
            if args.analyze() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(metrics) = MetricsCalculator::calculate_metrics(path, &content) {
                        print!(" - {}", MetricsCalculator::metrics_summary(&metrics));
                        
                        // Update totals
                        total_files += 1;
                        total_loc += metrics.lines_of_code;
                        total_comments += metrics.comment_lines;
                        total_functions += metrics.function_count as u64;
                        total_complexity += metrics.cyclomatic_complexity as u64;
                        analyzed_files += 1;
                    }
                }
            }
            
            println!(); // End the file line
            
            // Show diff if requested and file has changes
            if args.diff() && file_git_status.is_some() {
                match git_analyzer.get_semantic_diff(path) {
                    Ok(diff) => {
                        if !diff.trim().is_empty() {
                            println!("  ┌─ Diff:");
                            let lines: Vec<&str> = diff.lines().collect();
                            let lines_to_show = if args.truncate_diffs() && lines.len() > 10 {
                                &lines[..10]
                            } else {
                                &lines
                            };
                            
                            for line in lines_to_show {
                                println!("  │ {}", line);
                            }
                            
                            if args.truncate_diffs() && lines.len() > 10 {
                                println!("  │ ... (truncated, showing first 10 lines of {} total)", lines.len());
                            }
                            println!("  └─");
                        }
                    }
                    Err(e) => {
                        println!("  └─ Diff Error: {}", e);
                    }
                }
            }
            
            // Show diagnostics if requested (would need to implement file-level diagnostics)
            if args.diagnostics() {
                // For file-centric mode, we would need to run diagnostics per file
                // This is a simplified placeholder - the tree mode already has full diagnostics integration
                println!("  └─ (Diagnostics available in tree mode: --tree --diagnostics)");
            }
        }
        
        // Show summary statistics if analysis was performed
        if args.analyze() && analyzed_files > 0 {
            println!();
            println!("Summary Statistics:");
            println!("  Files analyzed: {}", analyzed_files);
            println!("  Total lines of code: {}", total_loc);
            println!("  Total comment lines: {}", total_comments);
            println!("  Total functions: {}", total_functions);
            println!("  Average complexity: {:.1}", 
                if total_functions > 0 { total_complexity as f64 / total_functions as f64 } else { 0.0 }
            );
        }
    }
    
    Ok(ExitCode::from(0))
}

/// Output comprehensive JSON that includes metadata and analysis information
async fn output_unified_json(
    tree: &crate::diagnostics::types::TreeNode,
    options: &crate::diagnostics::TreeDisplayOptions,
    args: &HiArgs,
    git_status: &std::collections::HashMap<std::path::PathBuf, crate::diagnostics::GitFileStatus>
) {
    use crate::diagnostics::TreeDisplay;
    
    // Create the main output structure
    let mut output = serde_json::Map::new();
    
    // Add metadata about the analysis
    let mut metadata = serde_json::Map::new();
    metadata.insert("version".to_string(), serde_json::Value::String("1.0".to_string()));
    metadata.insert("timestamp".to_string(), serde_json::Value::String(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string()
    ));
    metadata.insert("analysis_type".to_string(), serde_json::Value::String("unified_tree".to_string()));
    
    // Add absolute project root path
    if let Ok(absolute_root) = std::env::current_dir() {
        metadata.insert("project_root_absolute".to_string(), serde_json::Value::String(
            absolute_root.to_string_lossy().to_string()
        ));
    }
    
    // Add enabled features
    let mut features = serde_json::Map::new();
    features.insert("tree_view".to_string(), serde_json::Value::Bool(args.tree()));
    features.insert("diff_analysis".to_string(), serde_json::Value::Bool(args.diff()));
    features.insert("code_analysis".to_string(), serde_json::Value::Bool(args.analyze()));
    features.insert("compiler_diagnostics".to_string(), serde_json::Value::Bool(args.diagnostics()));
    features.insert("json_output".to_string(), serde_json::Value::Bool(args.json_output()));
    metadata.insert("enabled_features".to_string(), serde_json::Value::Object(features));
    
    // Add Git status summary
    if !git_status.is_empty() {
        let mut git_summary = serde_json::Map::new();
        let mut modified_count = 0;
        let mut staged_count = 0;
        let mut untracked_count = 0;
        let mut conflicted_count = 0;
        
        for status in git_status.values() {
            match status {
                crate::diagnostics::GitFileStatus::Modified => modified_count += 1,
                crate::diagnostics::GitFileStatus::Staged => staged_count += 1,
                crate::diagnostics::GitFileStatus::Untracked => untracked_count += 1,
                crate::diagnostics::GitFileStatus::Conflicted => conflicted_count += 1,
            }
        }
        
        git_summary.insert("total_changed_files".to_string(), serde_json::Value::Number(git_status.len().into()));
        git_summary.insert("modified".to_string(), serde_json::Value::Number(modified_count.into()));
        git_summary.insert("staged".to_string(), serde_json::Value::Number(staged_count.into()));
        git_summary.insert("untracked".to_string(), serde_json::Value::Number(untracked_count.into()));
        git_summary.insert("conflicted".to_string(), serde_json::Value::Number(conflicted_count.into()));
        
        metadata.insert("git_summary".to_string(), serde_json::Value::Object(git_summary));
    }
    
    output.insert("metadata".to_string(), serde_json::Value::Object(metadata));
    
    // Get the enhanced tree data from TreeDisplay
    let tree_data = TreeDisplay::create_enhanced_json(tree, options);
    output.insert("tree".to_string(), tree_data);
    
    // Output the complete JSON
    match serde_json::to_string_pretty(&output) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Error serializing unified JSON: {}", e),
    }
}
