use oxi_tarql::OxiTarql;
use std::fs;

#[test]
fn test_integration_split_with_custom_functions() {
    use std::path::PathBuf;

    // Create a temporary file for output
    let temp_file = std::env::temp_dir().join("oxi_tarql_test_output.nt");

    // Clean up any existing temp file
    let _ = std::fs::remove_file(&temp_file);

    // Get absolute paths for input files
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_path = manifest_dir.join("tests/fixtures/split.csv");
    let query_path = manifest_dir.join("tests/fixtures/splitfuncs.rq");

    // Verify test files exist
    assert!(input_path.exists(), "Input file should exist: {:?}", input_path);
    assert!(query_path.exists(), "Query file should exist: {:?}", query_path);

    let mut tarql = OxiTarql {
        delimiter: ",".to_string(),
        tab: false,
        test: 0,
        headers: false, // -H flag means "no-header-row", sets to false
        escape_char: "\\".to_string(),
        quote_char: "\"".to_string(),
        normalize: false,
        gzip: false,
        ntriples: false,
        quads: false,
        dedup: 0,
        named_graph: "".to_string(),
        input: input_path.to_str().unwrap().to_string(),
        output: temp_file.to_str().unwrap().to_string(),
        query: query_path.to_str().unwrap().to_string(),
        split: vec![
            ("d".to_string(), "d_s".to_string(), ";".to_string()),
            ("e".to_string(), "e_s".to_string(), " ".to_string()),
        ],
    };

    // Run the transformation
    let result = tarql.transform();
    assert!(result.is_ok(), "Transform should succeed: {:?}", result.err());

    // Read the output file and count triples
    assert!(temp_file.exists(), "Output file should exist at {:?}", temp_file);
    let content = fs::read_to_string(&temp_file).expect("Should read output file");

    let triple_count = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    // Verify we have exactly 55 triples
    // Row 1: d="1;2;3" (3 values) × e="A B C" (3 values) = 9 combinations
    // Row 2: d="4;5" (2 values) × e="D" (1 value) = 2 combinations
    // Total: 11 rows × 5 triples per row = 55 triples
    assert_eq!(triple_count, 55, "Expected 55 triples in output, got {}", triple_count);
}
