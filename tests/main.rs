use flate2::read::GzDecoder;
use oxi_gen::configure_transform;
use oxrdfio::RdfParser;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_integration_split_with_custom_functions() {
    // Create a temporary file for output
    let temp_file = std::env::temp_dir().join("oxi_gen_test_output.nt");

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
        "oxi_gen".to_string(),
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

    let mut transform = configure_transform(args);

    // Run the transformation
    let result = transform.transform();
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
    let temp_file = std::env::temp_dir().join("oxi_gen_test_output.ttl");

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
        "oxi_gen".to_string(),
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

    let mut transform = configure_transform(args);

    // Run the transformation
    let result = transform.transform();
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
    let temp_file = std::env::temp_dir().join("oxi_gen_test_dedup.nt.gz");

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
        "oxi_gen".to_string(),
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

    let mut transform = configure_transform(args);

    // Run the transformation
    let result = transform.transform();
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

#[test]
fn test_integration_optional_field_empty_values() {
    // Create a temporary file for output
    let temp_file = std::env::temp_dir().join("oxi_gen_test_optional.nt");

    // Clean up any existing temp file
    let _ = std::fs::remove_file(&temp_file);

    // Get absolute paths for input files
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_path = manifest_dir.join("tests/fixtures/optional_field.csv");
    let query_path = manifest_dir.join("tests/fixtures/optional_field.rq");

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

    // Build command-line arguments WITHOUT --bind-empty-strings (default behavior)
    let args = vec![
        "oxi_gen".to_string(),
        "--input".to_string(),
        input_path.to_str().unwrap().to_string(),
        "--query".to_string(),
        query_path.to_str().unwrap().to_string(),
        "--output".to_string(),
        temp_file.to_str().unwrap().to_string(),
        "--ntriples".to_string(),
    ];

    let mut transform = configure_transform(args);

    // Run the transformation
    let result = transform.transform();
    assert!(
        result.is_ok(),
        "Transform should succeed: {:?}",
        result.err()
    );

    // Read the output file
    assert!(
        temp_file.exists(),
        "Output file should exist at {:?}",
        temp_file
    );
    let content = fs::read_to_string(&temp_file).expect("Should read output file");

    // Count altLabel triples - should only be 1 (for the first row with "uno")
    let alt_label_count = content.matches("altLabel").count();

    // Count total non-empty lines (triples)
    let triple_count = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    // Verify only one altLabel triple was created (for row 1 with value "uno")
    assert_eq!(
        alt_label_count, 1,
        "Expected exactly 1 altLabel triple (only for row with non-empty value), got {}",
        alt_label_count
    );

    // Verify we have the expected number of triples:
    // Row 1: type, prefLabel, altLabel = 3 triples
    // Row 2: type, prefLabel (NO altLabel because value is empty) = 2 triples
    // Total: 5 triples
    assert_eq!(
        triple_count, 5,
        "Expected 5 triples total (3 for row 1, 2 for row 2), got {}",
        triple_count
    );

    // Verify the content contains expected values
    assert!(
        content.contains("\"uno\""),
        "Should contain the alt_label value 'uno' from first row"
    );
    assert!(
        content.contains("test.com/d/one"),
        "Should contain subject one from first row"
    );
    assert!(
        content.contains("test.com/d/two"),
        "Should contain subject two from second row"
    );
}

#[test]
fn test_integration_expand_prefixed_name_with_empty_values() {
    // Create a temporary file for output
    let temp_file = std::env::temp_dir().join("oxi_gen_test_successor.nt");

    // Clean up any existing temp file
    let _ = std::fs::remove_file(&temp_file);

    // Get absolute paths for input files
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_path = manifest_dir.join("tests/fixtures/successor_field.csv");
    let query_path = manifest_dir.join("tests/fixtures/successor_field.rq");

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

    // Build command-line arguments WITHOUT --bind-empty-strings (default behavior)
    let args = vec![
        "oxi_gen".to_string(),
        "--input".to_string(),
        input_path.to_str().unwrap().to_string(),
        "--query".to_string(),
        query_path.to_str().unwrap().to_string(),
        "--output".to_string(),
        temp_file.to_str().unwrap().to_string(),
        "--ntriples".to_string(),
    ];

    let mut transform = configure_transform(args);

    // Run the transformation
    let result = transform.transform();
    assert!(
        result.is_ok(),
        "Transform should succeed: {:?}",
        result.err()
    );

    // Read the output file
    assert!(
        temp_file.exists(),
        "Output file should exist at {:?}",
        temp_file
    );
    let content = fs::read_to_string(&temp_file).expect("Should read output file");

    // Count hasSuccessor triples - should only be 1 (for the first row with ":two")
    let successor_count = content.matches("hasSuccessor").count();

    // Count altLabel triples - should only be 1 (for the first row with "uno")
    let alt_label_count = content.matches("altLabel").count();

    // Count total non-empty lines (triples)
    let triple_count = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    // Verify only one hasSuccessor triple was created (for row 1 with value ":two")
    assert_eq!(
        successor_count, 1,
        "Expected exactly 1 hasSuccessor triple (only for row with non-empty successor), got {}",
        successor_count
    );

    // Verify only one altLabel triple was created (for row 1 with value "uno")
    assert_eq!(
        alt_label_count, 1,
        "Expected exactly 1 altLabel triple (only for row with non-empty alt_label), got {}",
        alt_label_count
    );

    // Verify we have the expected number of triples:
    // Row 1: type, prefLabel, altLabel, hasSuccessor = 4 triples
    // Row 2: type, prefLabel (NO altLabel, NO hasSuccessor because both are empty) = 2 triples
    // Total: 6 triples
    assert_eq!(
        triple_count, 6,
        "Expected 6 triples total (4 for row 1, 2 for row 2), got {}",
        triple_count
    );

    // Verify the content contains expected values
    assert!(
        content.contains("\"uno\""),
        "Should contain the alt_label value 'uno' from first row"
    );
    assert!(
        content.contains("test.com/d/one"),
        "Should contain subject one from first row"
    );
    assert!(
        content.contains("test.com/d/two"),
        "Should contain both subject two and successor reference to two"
    );
}

#[test]
fn test_integration_escaped_special_characters() {
    let temp_file = std::env::temp_dir().join("oxi_gen_test_escaped.nt");
    let _ = std::fs::remove_file(&temp_file);

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_path = manifest_dir.join("tests/fixtures/escaped_chars.csv");
    let query_path = manifest_dir.join("tests/fixtures/escaped_chars.rq");

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

    let args = vec![
        "oxi_gen".to_string(),
        "--input".to_string(),
        input_path.to_str().unwrap().to_string(),
        "--query".to_string(),
        query_path.to_str().unwrap().to_string(),
        "--output".to_string(),
        temp_file.to_str().unwrap().to_string(),
        "--ntriples".to_string(),
    ];

    let mut transform = configure_transform(args);
    let result = transform.transform();
    assert!(
        result.is_ok(),
        "Transform should succeed: {:?}",
        result.err()
    );

    assert!(
        temp_file.exists(),
        "Output file should exist at {:?}",
        temp_file
    );

    // Parse the N-Triples output with a proper RDF parser to validate correctness
    let file = fs::File::open(&temp_file).expect("Should open output file");
    let parser = RdfParser::from_format(oxrdfio::RdfFormat::NTriples).for_reader(file);

    let description_pred = "https://test.com/d/description";
    let mut descriptions: HashMap<String, String> = HashMap::new();

    for q in parser {
        let quad = q.expect("All output triples must be valid N-Triples");
        if quad.predicate.as_str() == description_pred
            && let oxrdf::Term::Literal(lit) = &quad.object
        {
            let subj = quad.subject.to_string();
            descriptions.insert(subj, lit.value().to_string());
        }
    }

    let _ = std::fs::remove_file(&temp_file);

    // Should have 4 rows, each producing a description triple
    assert_eq!(
        descriptions.len(),
        4,
        "Expected 4 description literals, got {}",
        descriptions.len()
    );

    // Row 1: CSV `\\` escapes produce literal backslashes in the value.
    // Verifies that backslash characters survive CSV→SPARQL→RDF serialization→parse round-trip.
    let row0 = descriptions
        .get("<https://test.com/d/row_0>")
        .expect("Should have description for row 0");
    assert_eq!(
        row0, "C:\\Users\\test\\path",
        "Row 0: backslash-escaped path should produce literal backslashes"
    );

    // Row 2: literal `\n` and `\t` sequences (not control characters).
    // The CSV escape char consumes the first `\`, so `\\n` → `\n` as two chars.
    let row1 = descriptions
        .get("<https://test.com/d/row_1>")
        .expect("Should have description for row 1");
    assert_eq!(
        row1, "line1\\nline2\\ttab",
        "Row 1: literal backslash-n and backslash-t sequences should be preserved"
    );
    // Confirm these are actual backslash + letter, not control characters
    assert!(
        !row1.contains('\n') && !row1.contains('\t'),
        "Row 1 must not contain real newline or tab control characters"
    );

    // Row 3: CSV `\"` escape produces literal quote characters in the value.
    let row2 = descriptions
        .get("<https://test.com/d/row_2>")
        .expect("Should have description for row 2");
    assert_eq!(
        row2, "say \"hello world\"",
        "Row 2: escaped quotes should produce literal double-quote characters"
    );

    // Row 4: both backslash and quote escapes together in one value.
    let row3 = descriptions
        .get("<https://test.com/d/row_3>")
        .expect("Should have description for row 3");
    assert_eq!(
        row3, "mixed: C:\\path \"quoted\"",
        "Row 3: mixed backslash and quote escapes should both be preserved"
    );
    assert!(
        row3.contains('\\') && row3.contains('"'),
        "Row 3 must contain both backslash and quote characters"
    );
}
