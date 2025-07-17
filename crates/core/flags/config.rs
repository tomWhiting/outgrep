/*!
This module provides routines for reading ripgrep config "rc" files.

The primary output of these routines is a sequence of arguments, where each
argument corresponds precisely to one shell argument.

This module supports hierarchical configuration loading with the following precedence:
1. CLI arguments (highest priority)
2. Local/project configuration files
3. Global configuration files
4. Legacy RIPGREP_CONFIG_PATH (lowest priority)
*/

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use bstr::{io::BufReadExt, ByteSlice};

use crate::flags::hierarchy::ConfigHierarchy;

/// Return a sequence of arguments derived from ripgrep rc configuration files.
/// 
/// This function implements hierarchical configuration loading:
/// 1. Global config files (lowest priority)
/// 2. Local/project config files (medium priority)  
/// 3. Legacy RIPGREP_CONFIG_PATH (for backward compatibility)
///
/// The returned arguments should be merged with CLI arguments, with CLI taking
/// the highest priority.
pub fn args() -> Vec<OsString> {
    let mut all_args = Vec::new();

    // Load hierarchical configuration (global + local)
    match ConfigHierarchy::load() {
        Ok(hierarchy) => {
            // Add global config args first
            if let Some(ref global) = hierarchy.global_config {
                log::debug!(
                    "{}: arguments loaded from global config: {:?}",
                    global.path.display(),
                    global.args
                );
                all_args.extend(global.args.clone());
            }

            // Add local config args second (higher priority)
            if let Some(ref local) = hierarchy.local_config {
                log::debug!(
                    "{}: arguments loaded from local config: {:?}",
                    local.path.display(),
                    local.args
                );
                all_args.extend(local.args.clone());
            }
        }
        Err(err) => {
            log::debug!("Failed to load hierarchical config: {}", err);
        }
    }

    // For backward compatibility, also check RIPGREP_CONFIG_PATH
    // This has the lowest priority, so it goes first
    let legacy_args = load_legacy_config();
    if !legacy_args.is_empty() {
        // Insert legacy args at the beginning (lowest priority)
        let mut combined_args = legacy_args;
        combined_args.extend(all_args);
        all_args = combined_args;
    }

    all_args
}

/// Load configuration from legacy RIPGREP_CONFIG_PATH for backward compatibility
fn load_legacy_config() -> Vec<OsString> {
    let config_path = match std::env::var_os("RIPGREP_CONFIG_PATH") {
        None => return vec![],
        Some(config_path) => {
            if config_path.is_empty() {
                return vec![];
            }
            PathBuf::from(config_path)
        }
    };
    let (args, errs) = match parse(&config_path) {
        Ok((args, errs)) => (args, errs),
        Err(err) => {
            message!(
                "failed to read the file specified in RIPGREP_CONFIG_PATH: {}",
                err
            );
            return vec![];
        }
    };
    if !errs.is_empty() {
        for err in errs {
            message!("{}:{}", config_path.display(), err);
        }
    }
    log::debug!(
        "{}: arguments loaded from legacy config file: {:?}",
        config_path.display(),
        args
    );
    args
}

/// Parse a single ripgrep rc file from the given path.
///
/// On success, this returns a set of shell arguments, in order, that should
/// be pre-pended to the arguments given to ripgrep at the command line.
///
/// If the file could not be read, then an error is returned. If there was
/// a problem parsing one or more lines in the file, then errors are returned
/// for each line in addition to successfully parsed arguments.
fn parse<P: AsRef<Path>>(
    path: P,
) -> anyhow::Result<(Vec<OsString>, Vec<anyhow::Error>)> {
    let path = path.as_ref();
    match std::fs::File::open(&path) {
        Ok(file) => parse_reader(file),
        Err(err) => anyhow::bail!("{}: {}", path.display(), err),
    }
}

/// Parse a single ripgrep rc file from the given reader.
///
/// Callers should not provided a buffered reader, as this routine will use its
/// own buffer internally.
///
/// On success, this returns a set of shell arguments, in order, that should
/// be pre-pended to the arguments given to ripgrep at the command line.
///
/// If the reader could not be read, then an error is returned. If there was a
/// problem parsing one or more lines, then errors are returned for each line
/// in addition to successfully parsed arguments.
fn parse_reader<R: std::io::Read>(
    rdr: R,
) -> anyhow::Result<(Vec<OsString>, Vec<anyhow::Error>)> {
    let mut bufrdr = std::io::BufReader::new(rdr);
    let (mut args, mut errs) = (vec![], vec![]);
    let mut line_number = 0;
    bufrdr.for_byte_line_with_terminator(|line| {
        line_number += 1;

        let line = line.trim();
        if line.is_empty() || line[0] == b'#' {
            return Ok(true);
        }
        match line.to_os_str() {
            Ok(osstr) => {
                args.push(osstr.to_os_string());
            }
            Err(err) => {
                errs.push(anyhow::anyhow!("{line_number}: {err}"));
            }
        }
        Ok(true)
    })?;
    Ok((args, errs))
}

#[cfg(test)]
mod tests {
    use super::parse_reader;
    use std::ffi::OsString;

    #[test]
    fn basic() {
        let (args, errs) = parse_reader(
            &b"\
# Test
--context=0
   --smart-case
-u


   # --bar
--foo
"[..],
        )
        .unwrap();
        assert!(errs.is_empty());
        let args: Vec<String> =
            args.into_iter().map(|s| s.into_string().unwrap()).collect();
        assert_eq!(args, vec!["--context=0", "--smart-case", "-u", "--foo",]);
    }

    // We test that we can handle invalid UTF-8 on Unix-like systems.
    #[test]
    #[cfg(unix)]
    fn error() {
        use std::os::unix::ffi::OsStringExt;

        let (args, errs) = parse_reader(
            &b"\
quux
foo\xFFbar
baz
"[..],
        )
        .unwrap();
        assert!(errs.is_empty());
        assert_eq!(
            args,
            vec![
                OsString::from("quux"),
                OsString::from_vec(b"foo\xFFbar".to_vec()),
                OsString::from("baz"),
            ]
        );
    }

    // ... but test that invalid UTF-8 fails on Windows.
    #[test]
    #[cfg(not(unix))]
    fn error() {
        let (args, errs) = parse_reader(
            &b"\
quux
foo\xFFbar
baz
"[..],
        )
        .unwrap();
        assert_eq!(errs.len(), 1);
        assert_eq!(args, vec![OsString::from("quux"), OsString::from("baz"),]);
    }
}
