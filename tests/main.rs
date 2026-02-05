use flate2::read::GzDecoder;
use oxi_tarql::configure_transform;
use oxrdfio::RdfParser;
use std::fs;
use std::path::PathBuf;

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
    assert!(
        input_path.exists(),
        "Input file should exist: {:?}",
        input_path
    );
    assert!(
        query_path.exists(),
        "Query file should exist: {:?}",
        query_path
    );

    // Build command-line arguments for configure_transform
    let args = vec![
        "oxi_tarql".to_string(),
        "--input".to_string(),
        input_path.to_str().unwrap().to_string(),
        "--query".to_string(),
        query_path.to_str().unwrap().to_string(),
        "--output".to_string(),
        temp_file.to_str().unwrap().to_string(),
        "-H".to_string(), // no-header-row flag
        "--ntriples".to_string(),
        "--split".to_string(),
        "d".to_string(),
        "d_s".to_string(),
        ";".to_string(),
        "--split".to_string(),
        "e".to_string(),
        "e_s".to_string(),
        " ".to_string(),
    ];

    let mut tarql = configure_transform(args);

    // Run the transformation
    let result = tarql.transform();
    assert!(
        result.is_ok(),
        "Transform should succeed: {:?}",
        result.err()
    );

    // Read the output file and count triples
    assert!(
        temp_file.exists(),
        "Output file should exist at {:?}",
        temp_file
    );
    let content = fs::read_to_string(&temp_file).expect("Should read output file");

    let triple_count = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    // Verify we have exactly 15 triples, since triples are deduplicated per row
    assert_eq!(
        triple_count, 15,
        "Expected 15 triples in output, got {}",
        triple_count
    );
}

#[test]
fn test_integration_turtle_serialization() {
    // Create a temporary file for output
    let temp_file = std::env::temp_dir().join("oxi_tarql_test_output.ttl");

    // Clean up any existing temp file
    let _ = std::fs::remove_file(&temp_file);

    // Get absolute paths for input files
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_path = manifest_dir.join("tests/fixtures/split.csv");
    let query_path = manifest_dir.join("tests/fixtures/splitfuncs.rq");

    // Verify test files exist
    assert!(
        input_path.exists(),
        "Input file should exist: {:?}",
        input_path
    );
    assert!(
        query_path.exists(),
        "Query file should exist: {:?}",
        query_path
    );

    // Build command-line arguments for configure_transform
    let args = vec![
        "oxi_tarql".to_string(),
        "--input".to_string(),
        input_path.to_str().unwrap().to_string(),
        "--query".to_string(),
        query_path.to_str().unwrap().to_string(),
        "--output".to_string(),
        temp_file.to_str().unwrap().to_string(),
        "-H".to_string(), // no-header-row flag
        "--dedup=1000".to_string(),
        "--split".to_string(),
        "d".to_string(),
        "d_s".to_string(),
        ";".to_string(),
        "--split".to_string(),
        "e".to_string(),
        "e_s".to_string(),
        " ".to_string(),
    ];

    let mut tarql = configure_transform(args);

    // Run the transformation
    let result = tarql.transform();
    assert!(
        result.is_ok(),
        "Transform should succeed: {:?}",
        result.err()
    );

    // Read the output file and count triples
    assert!(
        temp_file.exists(),
        "Output file should exist at {:?}",
        temp_file
    );
    let content = fs::read_to_string(&temp_file).expect("Should read output file");

    // Prefixes should be emitted only once
    assert!(content.matches("@prefix").count() == 3);
    // Validate subject sort
    assert!(content.find(":0 a :Item").unwrap() < content.find(":1 a :Item").unwrap());
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
    assert!(
        input_path.exists(),
        "Input file should exist: {:?}",
        input_path
    );
    assert!(
        query_path.exists(),
        "Query file should exist: {:?}",
        query_path
    );

    // Build command-line arguments for configure_transform
    let args = vec![
        "oxi_tarql".to_string(),
        "--input".to_string(),
        input_path.to_str().unwrap().to_string(),
        "--query".to_string(),
        query_path.to_str().unwrap().to_string(),
        "--output".to_string(),
        temp_file.to_str().unwrap().to_string(),
        "--ntriples".to_string(),
        "--gzip".to_string(),
        "--dedup=1000".to_string(),
    ];

    let mut tarql = configure_transform(args);

    // Run the transformation
    let result = tarql.transform();
    assert!(
        result.is_ok(),
        "Transform should succeed: {:?}",
        result.err()
    );

    // Verify output file exists
    assert!(
        temp_file.exists(),
        "Output file should exist at {:?}",
        temp_file
    );

    // Read and decompress the gzipped file
    let file = fs::File::open(&temp_file).expect("Should open gzipped output file");
    let decoder = GzDecoder::new(file);
    let parser = RdfParser::from_format(oxrdfio::RdfFormat::NTriples).for_reader(decoder);
    // Parse the RDF content to verify it's valid
    // Simply count lines to verify triples, and parse specific subjects
    let mut triples = 0;
    let mut fixed_meta_count = 0;
    for q in parser {
        let quad = q.expect("Failed to parse NTriples output");
        triples += 1;
        if quad.subject.is_named_node()
            && quad.subject.to_string() == "<https://test.com/d/FixedMeta>"
        {
            fixed_meta_count += 1;
        }
    }
    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    // Verify exactly 2 triples for :FixedMeta (type and prefLabel)
    assert_eq!(
        fixed_meta_count, 2,
        "Expected exactly 2 triples for :FixedMeta subject, got {}",
        fixed_meta_count
    );

    // Verify total triple count (2 for FixedMeta + 4 per row * 100 rows = 402)
    // With deduplication, the :FixedMeta triples should appear only once
    assert!(
        triples == 402,
        "Should have generated triples for all input rows"
    );

    eprintln!("Total triples generated: {}", triples);
    eprintln!(":FixedMeta triples: {}", fixed_meta_count);
}
