use std::{
    env, fs,
    process::{exit, Command},
    thread,
    time::Duration,
};

use clap::{Parser, Subcommand};
use itertools::Itertools;
use log::warn;
use mergiraf::{
    attempts::AttemptsCache,
    bug_reporter::report_bug,
    line_merge_and_structured_resolution, resolve_merge_cascading,
    settings::{imitate_cr_lf_from_input, normalize_to_lf, DisplaySettings},
    supported_langs::supported_languages,
};

const DISABLING_ENV_VAR_LEGACY: &str = "MERGIRAF_DISABLE";
const DISABLING_ENV_VAR: &str = "mergiraf";

/// Syntax-aware merge driver for Git.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct CliArgs {
    /// Write debug files to a particular directory to analyze
    /// the internal aspects of the merge
    #[clap(short, long = "debug", global = true)]
    debug_dir: Option<String>,
    /// Verbosity
    #[clap(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Do a three-way merge
    Merge {
        /// The path to the file containing the base revision
        base: String,
        /// The path to the file containing the left revision
        left: String,
        /// The path to the file containing the right revision
        right: String,
        /// Only attempt to merge the files by solving textual conflicts,
        /// without doing a full structured merge from the ground up.
        #[clap(long)]
        fast: bool,
        /// Display compact conflicts, breaking down lines
        #[arg(short, long, default_value_t = false)]
        compact: bool,
        /// Behave as a git merge driver: overwrite the left revision
        #[clap(short, long)]
        git: bool,
        /// The path to the file to write the merge result to
        #[clap(short, long, conflicts_with = "git")]
        output: Option<String>,
        /// Final path in which the merged result will be stored.
        /// It is used to detect the language of the files using the file extension.
        #[clap(short, long)]
        path_name: Option<String>,
        /// Name to use for the base revision in conflict markers
        #[clap(short = 's', long)]
        // the choice of 's' is inherited from Git's merge driver interface
        base_name: Option<String>,
        /// Name to use for the left revision in conflict markers
        #[clap(short = 'x', long)]
        // the choice of 'x' is inherited from Git's merge driver interface
        left_name: Option<String>,
        /// Name to use for the right revision in conflict markers
        #[clap(short = 'y', long)]
        // the choice of 'y' is inherited from Git's merge driver interface
        right_name: Option<String>,
        /// Maximum number of milliseconds to try doing the merging for, after which we fall back on git's own algorithm. Set to 0 to disable this limit.
        #[clap(short, long, default_value_t = 10000)]
        timeout: u64,
    },
    /// Solve the conflicts in a merged file
    Solve {
        /// Path to a file containing merge conflicts
        conflicts: String,
        /// Display compact conflicts, breaking down lines
        #[clap(short, long, default_value_t = false)]
        compact: bool,
        /// Keep file untouched and show the results of resolution on standard output instead
        #[clap(short, long)]
        keep: bool,
    },
    /// Review the resolution of a merge by showing the differences with a line-based merge
    Review {
        /// Identifier of the merge case
        merge_id: String,
    },
    /// Create a bug report for a bad merge
    Report {
        /// Identifier of the merge case (if it did not return conflicts) or path to file with merge conflicts
        merge_id_or_file: String,
    },
    /// Show the supported languages
    Languages {
        /// Print the list in a format suitable for inclusion in gitattributes
        #[arg(long, default_value_t = false)]
        gitattributes: bool,
    },
}

fn main() {
    let args = CliArgs::parse();

    match real_main(args) {
        Ok(exit_code) => exit(exit_code),
        Err(error) => {
            eprintln!("Mergiraf: {error}");
            exit(-1)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn do_merge(
    base: &'static str,
    left: &'static str,
    right: &'static str,
    fast: bool,
    path_name: Option<String>,
    timeout: Duration,
    settings: DisplaySettings<'static>,
    debug_dir: Option<&'static str>,
) -> Result<(i32, String), String> {
    let (tx, rx) = oneshot::channel();

    thread::spawn(move || {
        let res = || {
            let fname_base = &base;
            let contents_base = normalize_to_lf(&read_file_to_string(fname_base)?);
            let fname_left = &left;
            let original_contents_left = read_file_to_string(fname_left)?;
            let contents_left = normalize_to_lf(&original_contents_left);
            let fname_right = &right;
            let contents_right = normalize_to_lf(&read_file_to_string(fname_right)?);

            let attempts_cache = AttemptsCache::new(None, None).ok();

            let fname_base = path_name.as_deref().unwrap_or(fname_base);

            let merge_result = line_merge_and_structured_resolution(
                &contents_base,
                &contents_left,
                &contents_right,
                fname_base,
                &settings,
                !fast,
                attempts_cache.as_ref(),
                debug_dir,
            );

            let merge_output =
                imitate_cr_lf_from_input(&original_contents_left, &merge_result.contents);

            if merge_result.conflict_count > 0 {
                let old_git_detected = settings.base_revision_name == "%S";
                if old_git_detected {
                    warn!("Using Git v2.44.0 or above is recommended to get meaningful revision names on conflict markers when using Mergiraf.");
                }
                Ok((1, merge_output))
            } else {
                Ok((0, merge_output))
            }
        };
        let _ = tx.send(res());
    });

    if timeout.is_zero() {
        rx.recv().unwrap()
    } else {
        rx.recv_timeout(timeout).map_err(|err| match err {
            oneshot::RecvTimeoutError::Timeout => {
                "structured merge took too long, falling back to Git"
            }
            oneshot::RecvTimeoutError::Disconnected => unreachable!(),
        })?
    }
}

fn real_main(args: CliArgs) -> Result<i32, String> {
    stderrlog::new()
        .module(module_path!())
        .verbosity(if args.verbose { 3 } else { 2 })
        .init()
        .unwrap();

    let default_base_name = "base";
    let default_left_name = "left";
    let default_right_name = "right";

    let return_code = match args.command {
        CliCommand::Merge {
            base,
            left,
            right,
            fast,
            path_name,
            git,
            output,
            base_name,
            left_name,
            right_name,
            compact,
            timeout,
        } => {
            let base: &'static str = base.leak();
            let left: &'static str = left.leak();
            let right: &'static str = right.leak();

            let debug_dir: Option<&'static str> = args.debug_dir.map(String::leak).map(|s| &*s);

            let settings: DisplaySettings<'static> = DisplaySettings {
                diff3: true,
                compact,
                conflict_marker_size: 7,
                base_revision_name: match base_name {
                    Some(s) if s == "%S" => default_base_name,
                    Some(name) => name.leak(),
                    None => base,
                },
                left_revision_name: match left_name {
                    Some(s) if s == "%X" => default_left_name,
                    Some(name) => name.leak(),
                    None => left,
                },
                right_revision_name: match right_name {
                    Some(s) if s == "%Y" => default_right_name,
                    Some(name) => name.leak(),
                    None => right,
                },
            };

            {
                let mergiraf_disabled = env::var(DISABLING_ENV_VAR).is_ok_and(|v| v == "0")
                    || env::var(DISABLING_ENV_VAR_LEGACY).is_ok_and(|v| !v.is_empty()); // TODO: deprecate

                if mergiraf_disabled {
                    return fallback_to_git_merge_file(base, left, right, git, &settings);
                }
            }

            let timeout = Duration::from_millis(timeout);

            match do_merge(
                base,
                left,
                right,
                fast,
                path_name,
                timeout,
                settings.clone(),
                debug_dir,
            ) {
                Ok((return_code, merge_output)) => {
                    if let Some(fname_out) = output {
                        write_string_to_file(&fname_out, &merge_output)?
                    } else if git {
                        write_string_to_file(left, &merge_output)?
                    } else {
                        print!("{merge_output}");
                    };
                    return_code
                }
                Err(err) => {
                    log::error!("Mergiraf: {err}");
                    return fallback_to_git_merge_file(base, left, right, git, &settings);
                }
            }
        }
        CliCommand::Solve {
            conflicts: fname_conflicts,
            compact,
            keep,
        } => {
            let settings = DisplaySettings {
                diff3: true,
                compact,
                conflict_marker_size: 7,
                base_revision_name: default_base_name, // TODO detect from file
                left_revision_name: default_left_name,
                right_revision_name: default_right_name,
            };

            let original_conflict_contents = read_file_to_string(&fname_conflicts)?;
            let conflict_contents = normalize_to_lf(&original_conflict_contents);
            let working_dir = env::current_dir().expect("Invalid current directory");

            let postprocessed = resolve_merge_cascading(
                &conflict_contents,
                &fname_conflicts.clone(),
                settings,
                args.debug_dir.as_deref(),
                &working_dir,
            );
            match postprocessed {
                Ok(merged) if merged.method == "original" => 1,
                Ok(merged) => {
                    if keep {
                        print!(
                            "{}",
                            imitate_cr_lf_from_input(&original_conflict_contents, &merged.contents)
                        );
                    } else {
                        write_string_to_file(&fname_conflicts, &merged.contents)?;
                        write_string_to_file(&(fname_conflicts + ".orig"), &conflict_contents)?;
                    };
                    0
                }
                Err(e) => {
                    warn!("Mergiraf: {e}");
                    1
                }
            }
        }
        CliCommand::Review { merge_id } => {
            let attempts_cache = AttemptsCache::new(None, None)?;
            attempts_cache.review_merge(&merge_id)?;
            0
        }
        CliCommand::Languages { gitattributes } => {
            for lang_profile in supported_languages() {
                if gitattributes {
                    for extension in lang_profile.extensions {
                        println!("*{extension} merge=mergiraf");
                    }
                } else {
                    println!(
                        "{} ({})",
                        lang_profile.name,
                        lang_profile
                            .extensions
                            .iter()
                            .map(|ext| format!("*{ext}"))
                            .join(", ")
                    );
                }
            }
            0
        }
        CliCommand::Report { merge_id_or_file } => {
            report_bug(merge_id_or_file)?;
            0
        }
    };
    Ok(return_code)
}

fn read_file_to_string(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("Could not read {path}: {err}"))
}

fn write_string_to_file(path: &str, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|err| format!("Could not write {path}: {err}"))
}

fn fallback_to_git_merge_file(
    base: &str,
    left: &str,
    right: &str,
    git: bool,
    settings: &DisplaySettings,
) -> Result<i32, String> {
    let mut command = Command::new("git");
    command.arg("merge-file").arg("--diff-algorithm=histogram");
    if !git {
        command.arg("-p");
    }
    command
        .arg("-L")
        .arg(settings.left_revision_name)
        .arg("-L")
        .arg(settings.base_revision_name)
        .arg("-L")
        .arg(settings.right_revision_name)
        .arg(left)
        .arg(base)
        .arg(right)
        .spawn()
        .and_then(|mut process| {
            process
                .wait()
                .map(|exit_status| exit_status.code().unwrap_or(0))
        })
        .map_err(|err| err.to_string())
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    CliArgs::command().debug_assert();
}
