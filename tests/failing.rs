use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use diffy_imara::{PatchFormatter, create_patch};
use mergiraf::settings::DisplaySettings;
use mergiraf::{PathBufExt, line_merge_and_structured_resolution};
use rstest::rstest;

mod common;
use common::detect_extension;

#[derive(Clone, Copy)]
enum FailingTestResult {
    /// test failed in the expected manner
    FailsCorrectly,
    /// test failed to fail, and is now correct!
    NowCorrect,
    /// test failed, but in a new way
    FailsIncorrectly,
}

#[rstest]
fn integration_failing(#[files("examples/*/failing/*")] test_dir: PathBuf) {
    let ext = detect_extension(&test_dir);
    #[expect(unstable_name_collisions)]
    let fname_base = test_dir.join(format!("Base.{ext}")).leak();
    let contents_base = fs::read_to_string(&fname_base)
        .expect("Unable to read left file")
        .leak();
    let fname_left = test_dir.join(format!("Left.{ext}"));
    let contents_left = fs::read_to_string(fname_left)
        .expect("Unable to read left file")
        .leak();
    let fname_right = test_dir.join(format!("Right.{ext}"));
    let contents_right = fs::read_to_string(fname_right)
        .expect("Unable to read right file")
        .leak();

    let fname_expected_currently = test_dir.join(format!("ExpectedCurrently.{ext}"));
    let contents_expected_currently = fs::read_to_string(&fname_expected_currently)
        .expect("Unable to read expected-currently file");
    let fname_expected_ideally = test_dir.join(format!("ExpectedIdeally.{ext}"));
    let contents_expected_ideally =
        fs::read_to_string(fname_expected_ideally).expect("Unable to read expected-ideally file");

    let merge_result = line_merge_and_structured_resolution(
        contents_base,
        contents_left,
        contents_right,
        fname_base,
        DisplaySettings::default(),
        true,
        None,
        None,
        Duration::from_millis(0),
    );

    let actual = merge_result.contents.trim();

    let expected_currently = contents_expected_currently.trim();
    let expected_ideally = contents_expected_ideally.trim();

    let result = if expected_currently == expected_ideally {
        FailingTestResult::NowCorrect
    } else if actual == expected_currently {
        FailingTestResult::FailsCorrectly
    } else if actual == expected_ideally {
        FailingTestResult::NowCorrect
    } else {
        FailingTestResult::FailsIncorrectly
    };

    match result {
        FailingTestResult::FailsCorrectly => {
            // test failed in the expected manner
        }
        FailingTestResult::NowCorrect => {
            // if you find yourself seeing this message:
            // 1. move the test to `working` subdirectory
            // 2. rename `ExpectedIdeally.<extension>` to `Expected.<extension>`
            // 3. delete `ExpectedCurrently.<extension>`
            panic!(
                "test for {} failed to fail -- it works now!",
                test_dir.display()
            );
        }
        FailingTestResult::FailsIncorrectly => {
            let patch = create_patch(expected_currently, actual);
            let f = PatchFormatter::new().with_color();
            print!("{}", f.fmt_patch(&patch));
            eprintln!(
                "test for {} failed, but output differs from what we currently expect",
                test_dir.display(),
            );
            panic!();
        }
    }
}
