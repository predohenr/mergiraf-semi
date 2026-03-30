use diffy_imara as diffy;
use crate::settings::DisplaySettings;
use crate::parsed_merge::ParsedMerge;

///Textual merge result could be a conflict or not
#[derive(Debug, PartialEq, Eq)]
pub enum TextualMergeResult{
    Success(String),
    Conflict(String),
}

pub trait TextualMerger{
    fn merge(&self, base: &str, left: &str, right: &str) -> TextualMergeResult;
}

///diffy merge implementation
pub struct DiffyMerger;
impl TextualMerger for DiffyMerger {
    fn merge(&self, base: &str, left: &str, right: &str) -> TextualMergeResult {
        
        let result = diffy::merge(base, left, right);

        let merged_text = match result {
            Ok(t) => t,
            Err(t) => t,
        };

        let settings = DisplaySettings::default(); 

        match ParsedMerge::parse(&merged_text, &settings) {
            Ok(parsed) => {
                if parsed.conflict_count() > 0 {
                    TextualMergeResult::Conflict(merged_text)
                } else {
                    TextualMergeResult::Success(merged_text)
                }
            },
            Err(_) => {
                // parsing failed
                TextualMergeResult::Conflict(merged_text)
            }
        }
    }
}