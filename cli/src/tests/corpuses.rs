use super::allocations;
use super::fixtures::{fixtures_dir, get_language, get_test_language};
use crate::generate;
use crate::test::{parse_tests, print_diff, print_diff_key, TestEntry};
use crate::util;
use std::fs;
use tree_sitter::{Language, LogType, Parser};

const LANGUAGES: &'static [&'static str] = &[
    "bash",
    "c",
    "cpp",
    "embedded-template",
    "go",
    "html",
    "javascript",
    "python",
];

lazy_static! {
    static ref LANGUAGE_FILTER: Option<String> =
        std::env::var("TREE_SITTER_TEST_LANGUAGE_FILTER").ok();
    static ref EXAMPLE_FILTER: Option<String> =
        std::env::var("TREE_SITTER_TEST_EXAMPLE_FILTER").ok();
    static ref LOG_ENABLED: bool = std::env::var("TREE_SITTER_ENABLE_LOG").is_ok();
    static ref LOG_GRAPH_ENABLED: bool = std::env::var("TREE_SITTER_ENABLE_LOG_GRAPHS").is_ok();
}

#[test]
fn test_real_language_corpus_files() {
    let grammars_dir = fixtures_dir().join("grammars");

    let mut did_fail = false;
    for language_name in LANGUAGES.iter().cloned() {
        if let Some(filter) = LANGUAGE_FILTER.as_ref() {
            if language_name != filter.as_str() {
                continue;
            }
        }

        eprintln!("language: {:?}", language_name);

        let language = get_language(language_name);
        let corpus_dir = grammars_dir.join(language_name).join("corpus");
        let test = parse_tests(&corpus_dir).unwrap();
        did_fail |= run_mutation_tests(language, test);
    }

    if did_fail {
        panic!("Corpus tests failed");
    }
}

#[test]
fn test_error_corpus_files() {
    let corpus_dir = fixtures_dir().join("error_corpus");

    let mut did_fail = false;
    for entry in fs::read_dir(&corpus_dir).unwrap() {
        let entry = entry.unwrap();
        let language_name = entry.file_name();
        let language_name = language_name.to_str().unwrap().replace("_errors.txt", "");
        if let Some(filter) = LANGUAGE_FILTER.as_ref() {
            if language_name != filter.as_str() {
                continue;
            }
        }

        eprintln!("language: {:?}", language_name);

        let test = parse_tests(&entry.path()).unwrap();
        let language = get_language(&language_name);
        did_fail |= run_mutation_tests(language, test);
    }

    if did_fail {
        panic!("Corpus tests failed");
    }
}

#[test]
fn test_feature_corpus_files() {
    let test_grammars_dir = fixtures_dir().join("test_grammars");

    let mut did_fail = false;
    for entry in fs::read_dir(&test_grammars_dir).unwrap() {
        let entry = entry.unwrap();
        if !entry.metadata().unwrap().is_dir() {
            continue;
        }
        let language_name = entry.file_name();
        let language_name = language_name.to_str().unwrap();

        if let Some(filter) = LANGUAGE_FILTER.as_ref() {
            if language_name != filter.as_str() {
                continue;
            }
        }

        eprintln!("test language: {:?}", language_name);

        let test_path = entry.path();
        let grammar_path = test_path.join("grammar.json");
        let error_message_path = test_path.join("expected_error.txt");
        let grammar_json = fs::read_to_string(grammar_path).unwrap();
        let generate_result = generate::generate_parser_for_grammar(&grammar_json);

        if error_message_path.exists() {
            let expected_message = fs::read_to_string(&error_message_path).unwrap();
            if let Err(e) = generate_result {
                if e.0 != expected_message {
                    panic!(
                        "Unexpected error message.\n\nExpected:\n\n{}\nActual:\n\n{}\n",
                        expected_message, e.0
                    );
                }
            } else {
                panic!(
                    "Expected error message but got none for test grammar '{}'",
                    language_name
                );
            }
        } else {
            let corpus_path = test_path.join("corpus.txt");
            let c_code = generate_result.unwrap().1;
            let language = get_test_language(language_name, c_code, &test_path);
            let test = parse_tests(&corpus_path).unwrap();
            did_fail |= run_mutation_tests(language, test);
        }
    }

    if did_fail {
        panic!("Corpus tests failed");
    }
}

fn run_mutation_tests(language: Language, test: TestEntry) -> bool {
    match test {
        TestEntry::Example {
            name,
            input,
            output,
        } => {
            if let Some(filter) = EXAMPLE_FILTER.as_ref() {
                if !name.contains(filter.as_str()) {
                    return false;
                }
            }

            eprintln!("  example: {:?}", name);

            allocations::start_recording();
            let mut log_session = None;
            let mut parser = get_parser(&mut log_session, "log.html");
            parser.set_language(language).unwrap();
            let tree = parser
                .parse_utf8(&mut |byte_offset, _| &input[byte_offset..], None)
                .unwrap();
            let actual = tree.root_node().to_sexp();
            drop(tree);
            drop(parser);
            if actual != output {
                print_diff_key();
                print_diff(&actual, &output);
                println!("");
                true
            } else {
                allocations::stop_recording();
                false
            }
        }
        TestEntry::Group { children, .. } => {
            let mut result = false;
            for child in children {
                result |= run_mutation_tests(language, child);
            }
            result
        }
    }
}

fn get_parser(session: &mut Option<util::LogSession>, log_filename: &str) -> Parser {
    let mut parser = Parser::new();

    if *LOG_ENABLED {
        parser.set_logger(Some(Box::new(|log_type, msg| {
            if log_type == LogType::Lex {
                eprintln!("  {}", msg);
            } else {
                eprintln!("{}", msg);
            }
        })));
    } else if *LOG_GRAPH_ENABLED {
        *session = Some(util::log_graphs(&mut parser, log_filename).unwrap());
    }

    parser
}