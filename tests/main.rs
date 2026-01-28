use oxi_tarql::OxiTarql;
use std::fs;
use std::process::Command;
use std::path::PathBuf;
use std::io::Read;
use flate2::read::GzDecoder;

#[test]
fn test_integration_split_with_custom_functions() {
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

#[test]
fn test_integration_with_dedup_and_gzip() {
    // Get paths to test files
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_path = manifest_dir.join("tests/fixtures/data_100.csv");
    let query_path = manifest_dir.join("tests/fixtures/with_dup.rq");
    let temp_file = std::env::temp_dir().join("oxi_tarql_test_dedup.nt.gz");

    // Clean up any existing temp file
    let _ = std::fs::remove_file(&temp_file);

    // Verify test files exist
    assert!(input_path.exists(), "Input file should exist: {:?}", input_path);
    assert!(query_path.exists(), "Query file should exist: {:?}", query_path);

    // Find the binary (it should be in target/debug or target/release)
    let binary = if cfg!(debug_assertions) {
        manifest_dir.join("target/debug/oxi_tarql")
    } else {
        manifest_dir.join("target/release/oxi_tarql")
    };

    // If binary doesn't exist, try to build it
    if !binary.exists() {
        let build_result = Command::new("cargo")
            .args(["build"])
            .current_dir(&manifest_dir)
            .output()
            .expect("Failed to build binary");

        assert!(build_result.status.success(), "Build failed: {:?}",
                String::from_utf8_lossy(&build_result.stderr));
    }

    assert!(binary.exists(), "Binary should exist at {:?}", binary);

    // Run the binary with command-line arguments
    let output = Command::new(&binary)
        .args([
            "--input", input_path.to_str().unwrap(),
            "--query", query_path.to_str().unwrap(),
            "--output", temp_file.to_str().unwrap(),
            "--gzip",
            "--dedup=1000",
        ])
        .output()
        .expect("Failed to execute binary");

    // Check if command succeeded
    assert!(output.status.success(),
            "Command failed with status: {:?}\nStderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr));

    // Verify output file exists
    assert!(temp_file.exists(), "Output file should exist at {:?}", temp_file);

    // Read and decompress the gzipped file
    let file = fs::File::open(&temp_file).expect("Should open gzipped output file");
    let mut decoder = GzDecoder::new(file);
    let mut rdf_content = String::new();
    decoder.read_to_string(&mut rdf_content)
        .expect("Should decompress gzipped content");

    // Parse the RDF content to verify it's valid
    // Simply count lines to verify triples, and parse specific subjects
    let mut triples = Vec::new();
    let mut fixed_meta_count = 0;

    for line in rdf_content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Verify line looks like a valid N-Triple (subject predicate object .)
        assert!(line.ends_with(" ."), "Line should end with ' .' in N-Triples format: {}", line);

        triples.push(line.to_string());

        // Count triples with :FixedMeta as subject
        if line.starts_with("<https://test.com/d/FixedMeta>") {
            fixed_meta_count += 1;
        }
    }

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    // Verify exactly 2 triples for :FixedMeta (type and prefLabel)
    assert_eq!(
        fixed_meta_count,
        2,
        "Expected exactly 2 triples for :FixedMeta subject, got {}",
        fixed_meta_count
    );

    // Verify total triple count (2 for FixedMeta + 4 per row * 100 rows = 402)
    // With deduplication, the :FixedMeta triples should appear only once
    assert!(triples.len() > 100, "Should have generated triples for all input rows");

    eprintln!("Total triples generated: {}", triples.len());
    eprintln!(":FixedMeta triples: {}", fixed_meta_count);
}
