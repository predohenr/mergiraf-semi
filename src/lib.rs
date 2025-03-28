//! Syntax aware merging of diverging files
//!
//! ## Overview
//!
//! Mergiraf is a structured merge tool. It takes three versions of a file (base, left and right)
//! and produces a fourth version where the changes from base to left and from base to right are
//! added. It does so with awareness of the syntax of the files, unlike Git's built-in line-based
//! merge algorithm.
//!
//! It is primarily designed to be used as a CLI which implements Git merge driver.
//! This means that it can replace Git's default merge algorithm when merging or rebasing branches.
//!
//! ## Using as a library to build other programs
//!
//! Mergiraf is not designed to be used as a library so far, the Rust API is therefore not meant
//! to be stable.

pub mod attempts;
pub mod bug_reporter;
pub(crate) mod changeset;
pub(crate) mod class_mapping;
pub(crate) mod git;
pub mod lang_profile;
pub mod line_based;
pub(crate) mod matching;
mod merge;
pub(crate) mod merge_3dm;
pub(crate) mod merge_postprocessor;
pub(crate) mod merge_result;
pub(crate) mod merged_text;
pub(crate) mod merged_tree;
pub(crate) mod multimap;
pub mod newline;
pub(crate) mod parsed_merge;
mod path_buf_ext;
pub(crate) mod pcs;
pub(crate) mod priority_list;
pub mod settings;
pub(crate) mod signature;
pub(crate) mod structured;
pub mod supported_langs;
#[cfg(test)]
pub(crate) mod test_utils;
pub mod tree;
pub(crate) mod tree_builder;
pub(crate) mod tree_matcher;
pub(crate) mod visualizer;

use core::fmt::Write;
use std::{fs, path::Path, time::Instant};

use git::extract_revision_from_git;

use itertools::Itertools;
use lang_profile::LangProfile;
use log::{debug, info, warn};

use merge_result::MergeResult;
use parsed_merge::{PARSED_MERGE_DIFF2_DETECTED, ParsedMerge};
use pcs::Revision;
use settings::DisplaySettings;
use structured::structured_merge;
use supported_langs::SUPPORTED_LANGUAGES;
use tree::{Ast, AstNode};
use tree_sitter::Parser as TSParser;
use typed_arena::Arena;

pub use path_buf_ext::PathBufExt;

/// Current way to disable Mergiraf
/// ## Usage
/// ```console
/// mergiraf=0 mergiraf merge foo bar baz
/// ```
pub const DISABLING_ENV_VAR: &str = "mergiraf";

pub(crate) const FROM_PARSED_ORIGINAL: &str = "from_parsed_original";

pub use merge::line_merge_and_structured_resolution;

/// Helper to parse a source text with a given tree-sitter parser.
pub fn parse<'a>(
    parser: &mut TSParser,
    contents: &'a str,
    lang_profile: &LangProfile,
    arena: &'a Arena<AstNode<'a>>,
    ref_arena: &'a Arena<&'a AstNode<'a>>,
) -> Result<Ast<'a>, String> {
    let tree = parser
        .parse(contents, None)
        .expect("Parsing example source code failed");
    Ast::new(&tree, contents, lang_profile, arena, ref_arena)
}

/// Takes a vector of merge results produced by [`resolve_merge_cascading`] and picks the best one
fn select_best_solve(mut solves: Vec<MergeResult>) -> Result<MergeResult, String> {
    if solves.is_empty() {
        return Err("Could not generate any solution".to_string());
    }

    solves.sort_by_key(|solve| solve.conflict_mass);
    debug!("~~~ Solve statistics ~~~");
    for solve in &solves {
        debug!(
            "{}: {} conflict(s), {} mass, has_additional_issues: {}",
            solve.method, solve.conflict_count, solve.conflict_mass, solve.has_additional_issues
        );
    }

    let best_solve = solves
        .into_iter()
        .find_or_first(|solve| !solve.has_additional_issues)
        .expect("checked for non-emptiness above");

    if best_solve.method == FROM_PARSED_ORIGINAL {
        // the best solve we've got is the line-based one
        Err("Could not generate any solution".to_string())
    } else {
        Ok(best_solve)
    }
}

/// Takes the result of an earlier merge process (likely line-based)
/// and attempts to resolve the remaining conflicts using structured merge
/// on the enclosing AST nodes.
///
/// Returns either a merge (potentially with conflicts) or an error.
fn resolve_merge<'a>(
    parsed_merge: &ParsedMerge<'a>,
    settings: &DisplaySettings<'a>,
    lang_profile: &LangProfile,
    debug_dir: Option<&Path>,
) -> Result<MergeResult, String> {
    let start = Instant::now();

    let base_rev = parsed_merge.reconstruct_revision(Revision::Base);
    let left_rev = parsed_merge.reconstruct_revision(Revision::Left);
    let right_rev = parsed_merge.reconstruct_revision(Revision::Right);

    debug!(
        "re-constructing revisions from parsed merge took {:?}",
        start.elapsed()
    );

    structured_merge(
        &base_rev,
        &left_rev,
        &right_rev,
        Some(parsed_merge),
        settings,
        lang_profile,
        debug_dir,
    )
}

/// Extracts the original revisions of the file from Git and performs a fully structured merge (see
/// [`structured_merge`])
///
/// Returns either a merge or nothing if couldn't extract the revisions.
fn structured_merge_from_git_revisions(
    fname_base: &Path,
    settings: &DisplaySettings,
    debug_dir: Option<&Path>,
    working_dir: &Path,
    lang_profile: &LangProfile,
) -> Result<MergeResult, String> {
    let revision_base = extract_revision(working_dir, fname_base, Revision::Base);
    let revision_left = extract_revision(working_dir, fname_base, Revision::Left);
    let revision_right = extract_revision(working_dir, fname_base, Revision::Right);

    // we only attempt a full structured merge if we could extract revisions from Git
    match (revision_base, revision_left, revision_right) {
        (Ok(contents_base), Ok(contents_left), Ok(contents_right)) => structured_merge(
            &contents_base,
            &contents_left,
            &contents_right,
            None,
            settings,
            lang_profile,
            debug_dir,
        ),
        (rev_base, _, _) => {
            if let Err(b) = rev_base {
                println!("{b}");
            }
            Err("Could not retrieve conflict sides from Git.".to_owned())
        }
    }
}

/// Cascading merge resolution starting from a user-supplied file with merge conflicts
pub fn resolve_merge_cascading<'a>(
    merge_contents: &'a str,
    fname_base: &Path,
    mut settings: DisplaySettings<'a>,
    debug_dir: Option<&Path>,
    working_dir: &Path,
) -> Result<MergeResult, String> {
    let mut solves = Vec::with_capacity(3);

    let lang_profile = LangProfile::detect_from_filename(fname_base).ok_or_else(|| {
        format!(
            "Could not find a supported language for {}",
            fname_base.display()
        )
    })?;

    match ParsedMerge::parse(merge_contents, &settings) {
        Err(err) => {
            if err == PARSED_MERGE_DIFF2_DETECTED {
                // if parsing the original merge failed because it's done in diff2 mode,
                // then we warn the user about it but don't give up yet as we can try a full merge
                warn!(
                    "Cannot solve conflicts in diff2 style. Merging the original conflict sides from scratch instead."
                );
            } else {
                warn!(
                    "Error while parsing conflicts: {err}. Merging the original conflict sides from scratch instead."
                );
            }
        }
        Ok(parsed_merge) => {
            settings.add_revision_names(&parsed_merge);

            match resolve_merge(&parsed_merge, &settings, lang_profile, debug_dir) {
                Ok(solve) if solve.conflict_count == 0 => {
                    info!("Solved all conflicts.");
                    return Ok(solve);
                }
                Ok(solve) => solves.push(solve),
                Err(err) => warn!("Error while resolving conflicts: {err}"),
            }

            let rendered_from_parsed = MergeResult {
                contents: parsed_merge.render(&settings),
                conflict_count: parsed_merge.conflict_count(),
                conflict_mass: parsed_merge.conflict_mass(),
                method: FROM_PARSED_ORIGINAL,
                has_additional_issues: false,
            };
            solves.push(rendered_from_parsed);
        }
    }

    // if we didn't manage to solve all conflicts, try again by extracting the original revisions from Git
    match structured_merge_from_git_revisions(
        fname_base,
        &settings,
        debug_dir,
        working_dir,
        lang_profile,
    ) {
        Ok(structured_merge) => solves.push(structured_merge),
        Err(err) => warn!("Full structured merge failed: {err}"),
    }
    let best_solve = select_best_solve(solves)?;

    match best_solve.conflict_count {
        0 => info!("Solved all conflicts."),
        n => info!("{n} conflict(s) remaining."),
    }
    Ok(best_solve)
}

fn extract_revision(working_dir: &Path, path: &Path, revision: Revision) -> Result<String, String> {
    let temp_file = extract_revision_from_git(working_dir, path, revision)?;
    let contents = fs::read_to_string(temp_file.path()).map_err(|err| err.to_string())?;
    Ok(contents)
}

fn fxhasher() -> rustc_hash::FxHasher {
    use std::hash::BuildHasher;
    rustc_hash::FxBuildHasher.build_hasher()
}

/// The implementation of `mergiraf languages`.
///
/// Prints the list of supported languages,
/// either in the format understood by `.gitattributes`,
/// or in a more human-readable format.
pub fn languages(gitattributes: bool) -> String {
    let mut res = String::new();
    for lang_profile in &*SUPPORTED_LANGUAGES {
        if gitattributes {
            for extension in &lang_profile.extensions {
                let _ = writeln!(res, "*.{extension} merge=mergiraf");
            }
        } else {
            let _ = writeln!(
                res,
                "{} ({})",
                lang_profile.name,
                lang_profile
                    .extensions
                    .iter()
                    .format_with(", ", |ext, f| f(&format_args!("*.{ext}")))
            );
        }
    }
    res
}

#[cfg(test)]
mod test {
    use super::*;

    use std::collections::HashSet;

    #[test]
    fn languages_gitattributes() {
        let supported_langs = languages(true);
        // put both into sets to ignore ordering
        let supported_langs: HashSet<_> = supported_langs.lines().collect();
        let expected: HashSet<_> = include_str!("../doc/src/supported_langs.txt")
            .lines()
            .collect();
        assert_eq!(
            supported_langs,
            expected,
            "\
You were probably adding a language to Mergiraf (thanks!), but forgot to update the documentation.
Please update `doc/src/languages.md` and `doc/src/supported_langs.txt`.
The following extensions are missing from the documentation: {:?}",
            supported_langs.difference(&expected)
        );
    }
}
