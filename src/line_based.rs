use std::borrow::Cow;

use crate::{parse, TSParser};
use diffy_imara::{Algorithm, ConflictStyle, MergeOptions};
use log::info;
use typed_arena::Arena;

use crate::{
    attempts::Attempt, lang_profile::LangProfile, parsed_merge::ParsedMerge,
    settings::DisplaySettings,
};

/// A merged output (represented as a string) together with statistics
/// about the conflicts it contains.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MergeResult {
    /// The output of the merge (the file contents possibly with conflicts)
    pub contents: String,
    /// The number of conflicts
    pub conflict_count: usize,
    /// The sum of the sizes of conflicts
    pub conflict_mass: usize,
    /// A name for the merge, identifying with which technique it was produced
    pub method: &'static str,
    /// Indicates that there are known conflicts which haven't been marked as such (such as duplicate signatures)
    pub has_additional_issues: bool,
}

pub const LINE_BASED_METHOD: &str = "line_based";
pub const STRUCTURED_RESOLUTION_METHOD: &str = "structured_resolution";
pub const FULLY_STRUCTURED_METHOD: &str = "fully_structured";

/// Ensures a given string has a newline at the end.
pub(crate) fn with_final_newline(s: Cow<str>) -> Cow<str> {
    if s.ends_with('\n') {
        s
    } else {
        s + "\n"
    }
}

/// Perform a textual merge with the diff3 algorithm.
pub(crate) fn line_based_merge(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    settings: &DisplaySettings,
) -> MergeResult {
    let merge_options = MergeOptions {
        conflict_marker_length: settings.conflict_marker_size,
        style: if settings.diff3 {
            ConflictStyle::Diff3
        } else {
            ConflictStyle::Merge
        },
        algorithm: Algorithm::Histogram,
    };
    let merged = merge_options.merge(contents_base, contents_left, contents_right);
    let merged_contents = match merged {
        Ok(contents) | Err(contents) => contents,
    };
    let parsed_merge = ParsedMerge::parse(&merged_contents)
        .expect("diffy-imara returned a merge that we cannot parse the conflicts of");
    MergeResult {
        contents: parsed_merge.render(settings),
        conflict_count: parsed_merge.conflict_count(),
        conflict_mass: parsed_merge.conflict_mass(),
        method: LINE_BASED_METHOD,
        has_additional_issues: false,
    }
}

/// Do a line-based merge. If it is conflict-free, also check if it introduced any duplicate signatures,
/// in which case this is logged as an additional issue on the merge result.
pub(crate) fn line_based_merge_with_duplicate_signature_detection(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    settings: &DisplaySettings,
    lang_profile: Option<&LangProfile>,
) -> MergeResult {
    let mut line_based_merge = line_based_merge(
        &with_final_newline(Cow::from(contents_base)),
        &with_final_newline(Cow::from(contents_left)),
        &with_final_newline(Cow::from(contents_right)),
        settings,
    );

    if line_based_merge.conflict_count == 0 {
        // If we support this language, check that there aren't any signature conflicts in the line-based merge
        if let Some(lang_profile) = lang_profile {
            let mut parser = TSParser::new();
            parser
                .set_language(&lang_profile.language)
                .unwrap_or_else(|_| panic!("Error loading {} grammar", lang_profile.name));
            let arena = Arena::new();
            let ref_arena = Arena::new();
            let tree_left = parse(
                &mut parser,
                &line_based_merge.contents,
                lang_profile,
                &arena,
                &ref_arena,
            );

            if let Ok(ast) = tree_left {
                if lang_profile.has_signature_conflicts(ast.root()) {
                    line_based_merge.has_additional_issues = true;
                }
            }
        }
    }
    line_based_merge
}

impl MergeResult {
    /// Helper to store a merge result in an attempt
    pub(crate) fn store_in_attempt(&self, attempt: &Attempt) {
        attempt.write(self.method, &self.contents).ok();
    }

    /// Helper to store a merge result in an attempt
    pub(crate) fn mark_as_best_merge_in_attempt(
        &self,
        attempt: &Attempt,
        line_based_conflicts: usize,
    ) {
        attempt.write_best_merge_id(self.method).ok();
        if self.conflict_count == 0 && line_based_conflicts > 0 {
            match line_based_conflicts {
                1 => {
                    info!(
                        "Mergiraf: Solved 1 conflict. Review with: mergiraf review {}",
                        attempt.id()
                    );
                }
                n => {
                    info!(
                        "Mergiraf: Solved {n} conflicts. Review with: mergiraf review {}",
                        attempt.id()
                    );
                }
            }
        }
    }
}
