use crate::{
    attempts::Attempt, lang_profile::LangProfile, line_based::LINE_BASED_METHOD, parse,
    parsed_merge::ParsedMerge, pcs::Revision, settings::DisplaySettings,
};
use log::info;
use tree_sitter::Parser as TSParser;
use typed_arena::Arena;

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

impl MergeResult {
    /// Detect any syntax errors or duplicate signatures, updating the
    /// `has_additional_issues` flag accordingly.
    pub fn detect_syntax_and_signature_errors(
        self,
        parser: &mut TSParser,
        lang_profile: &LangProfile,
        settings: &DisplaySettings,
    ) -> Self {
        let mut revision_has_issues = |contents: &str| {
            let arena = Arena::new();
            let ref_arena = Arena::new();

            let tree = parse(parser, contents, lang_profile, &arena, &ref_arena);

            tree.map_or(true, |ast| lang_profile.has_signature_conflicts(ast.root()))
        };

        let has_additional_issues = if self.conflict_count == 0 {
            revision_has_issues(&self.contents)
        } else {
            let parsed_merge = ParsedMerge::parse(&self.contents, settings).unwrap_or_else(|err| {
                panic!(
                    "Cannot parse merge results of method {}: {}",
                    self.method, err
                )
            });

            [Revision::Base, Revision::Left, Revision::Right]
                .into_iter()
                .map(|rev| parsed_merge.reconstruct_revision(rev))
                .any(|contents| revision_has_issues(&contents))
        };
        Self {
            has_additional_issues,
            ..self
        }
    }

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
                1 => info!(
                    "Mergiraf: Solved 1 conflict. Review with: mergiraf review {}",
                    attempt.id()
                ),
                n => info!(
                    "Mergiraf: Solved {n} conflicts. Review with: mergiraf review {}",
                    attempt.id()
                ),
            }
        }
    }

    pub(crate) fn from_parsed_merge(
        parsed_merge: &ParsedMerge,
        settings: &DisplaySettings,
    ) -> Self {
        Self {
            contents: parsed_merge.render(settings),
            conflict_count: parsed_merge.conflict_count(),
            conflict_mass: parsed_merge.conflict_mass(),
            method: LINE_BASED_METHOD,
            // the line-based merge might have come from a non-syntax-aware tool,
            // and we cautiously assume that it does have issues
            has_additional_issues: true,
        }
    }
}
