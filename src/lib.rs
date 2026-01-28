use std::{error::Error, fs};

use oxigraph::model::*;
use oxigraph::sparql::QueryResults;
use oxrdfio::{RdfFormat, RdfSerializer};
use regex::Regex;
use spareval::QueryEvaluator;
use spargebra::SparqlParser;

use csv::ReaderBuilder;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write, stdout};
use std::process::exit;

use clap::{Arg, ArgAction, ArgGroup, ArgMatches, command, value_parser};

#[allow(dead_code)]
#[derive(Default)]
pub struct OxiTarql {
    pub delimiter: String,
    pub tab: bool,
    pub test: u32,
    pub headers: bool,
    pub escape_char: String,
    pub quote_char: String,
    pub normalize: bool,
    pub gzip: bool,
    pub ntriples: bool,
    pub quads: bool,
    pub dedup: u32,
    pub named_graph: String,
    pub input: String,
    pub output: String,
    pub query: String,
    pub split: Vec<(String, String, String)>,
}

impl OxiTarql {
    pub fn transform(&mut self) -> Result<(), Box<dyn Error>> {
        let empty_store = Dataset::new();
        let mut store = HashSet::<Triple>::new();

        let query_str = fs::read_to_string(&self.query).unwrap();
        let query = match SparqlParser::new()
            .with_prefix("tarql", "https://semanticarts.com/tarql/")?
            .parse_query(&query_str)
        {
            Ok(qr) => qr,
            Err(e) => {
                eprintln!("SPARQL Syntax Error in query: {:?}", e);
                exit(-1);
            }
        };

        // Open the output file. Will use the filename if given or STDOUT if not
        let mut out_writer: BufWriter<Box<dyn Write>> =
            BufWriter::new(match self.output.as_ref() {
                "STDOUT" => Box::new(stdout()) as Box<dyn Write>,
                _ => {
                    if self.gzip {
                        let out_fh = File::create(&self.output)?;
                        let out_gz = GzEncoder::new(out_fh, Compression::default());
                        Box::new(BufWriter::new(out_gz))
                    } else {
                        let out_fh = File::create(&self.output)?;
                        Box::new(BufWriter::new(out_fh))
                    }
                }
            });

        let prefixes = extract_prefixes(&query_str).to_owned();
        let p2 = prefixes.clone();
        let evaluator = QueryEvaluator::new()
            .with_custom_function(
                NamedNode::new("https://semanticarts.com/tarql/expandPrefix")?,
                move |args| args.first().map(|p| expand_prefix(&prefixes, p).unwrap()),
            )
            .with_custom_function(
                NamedNode::new("https://semanticarts.com/tarql/expandPrefixedName")?,
                move |args| args.first().map(|p| expand_prefixed_name(&p2, p).unwrap()),
            );
        // oxigraph does not allow for specifying variable substitution unless
        // the variable is referenced in the query. Extract anything that looks like
        // a variable identifier, and then filter out columns that are not used
        let query_vars = extract_variables(&query_str);

        // Create CSV reader based on command line options
        let file = BufReader::with_capacity(100000, File::open(&self.input)?);
        let mut rdr = ReaderBuilder::new()
            .has_headers(self.headers)
            .delimiter(match self.tab {
                true => b'\t',
                _ => self.delimiter.chars().next().unwrap() as u8,
            })
            .quote(self.quote_char.chars().next().unwrap() as u8)
            .escape(Some(self.escape_char.chars().next().unwrap() as u8))
            .from_reader(file);

        // Extract headers from the CSV, unless --no-header-row is used, in
        // which case columns are aliased to 'a'..'z', 'A'..'Z' (max 52 columns)
        let mut headers = Vec::new();
        if self.headers {
            let header = rdr.headers()?.clone();

            for field in &header {
                headers.push(clean_column(field, &self.normalize).to_string());
            }
        } else {
            let alphabet_column_names: Vec<String> = ('a'..='z')
                .chain('A'..='Z')
                .map(|c| c.to_string())
                .collect();

            headers = alphabet_column_names.clone();
        }

        let mut row = 0;
        for result in rdr.records() {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = match result {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error reading row {}: {:?}", row, e);
                    exit(-1);
                }
            };

            let unwrapped = self.apply_split(&record, &headers);
            for unwrapped_row in unwrapped {
                let mut prepared = evaluator.prepare(&query);
                for (varname, value) in unwrapped_row {
                    if query_vars.contains(&varname) {
                        prepared = prepared
                            .substitute_variable(Variable::new(varname)?, Literal::from(value));
                    }
                }
                if query_vars.contains("ROWNUM") {
                    prepared =
                        prepared.substitute_variable(Variable::new("ROWNUM")?, Literal::from(row));
                }

                let results = prepared.execute(&empty_store);
                if let QueryResults::Graph(triples) = results.unwrap() {
                    for triple in triples {
                        if self.dedup > 0 {
                            store.insert(triple?);
                        } else {
                            let _ = writeln!(out_writer, "{} .", triple?);
                        }
                    }
                }
            }

            // If deduplicating and hit limit, flush store to output
            if self.dedup > 0 && store.len() >= self.dedup.try_into().unwrap() {
                flush_store(&mut store, &mut out_writer)?;
            }

            row += 1;
            if self.test != 0 && row == self.test {
                break;
            }
        }

        // If deduplicating, flush remaining store to output
        if self.dedup > 0 && !store.is_empty() {
            flush_store(&mut store, &mut out_writer)?;
        }

        out_writer.flush().expect("Error flushing to output file");
        Ok(())
    }

    fn apply_split<'a>(
        &self,
        record: &'a csv::StringRecord,
        headers: &'a [String],
    ) -> Vec<Vec<(String, &'a str)>> {
        let mut bindings: Vec<Vec<(String, &'a str)>> =
            vec![headers.iter().cloned().zip(record.iter()).collect()];
        for (original, split, delimiter) in self.split.iter() {
            let original_idx = match headers.iter().position(|h| h == original) {
                None => continue,
                Some(idx) => idx,
            };
            let mut next_vals: Vec<Vec<(String, &str)>> = vec![];
            for val_set in bindings {
                let original_val = val_set[original_idx].1;
                for split_val in original_val.split(delimiter) {
                    let mut modified_row = val_set.clone();
                    modified_row.push((split.clone(), split_val));
                    next_vals.push(modified_row);
                }
            }
            bindings = next_vals;
        }
        bindings
    }
}

fn flush_store(
    store: &mut HashSet<Triple>,
    out_writer: &mut BufWriter<Box<dyn Write + 'static>>,
) -> Result<(), Box<dyn Error + 'static>> {
    let mut serializer = RdfSerializer::from_format(RdfFormat::NTriples).for_writer(Vec::new());
    for triple in store.iter() {
        serializer.serialize_triple(triple)?;
    }
    let rdf_str = serializer.finish().unwrap();
    let _ = out_writer.write_all(&rdf_str);
    store.clear();
    Ok(())
}

fn expand_prefix(prefixes: &HashMap<String, String>, prefix: &Term) -> Option<Term> {
    let prefix_name = match prefix {
        Term::Literal(l) => l.value().to_string(),
        _ => {
            eprintln!("Invalid argument passed to expandPrefix: {:?}", prefix);
            exit(-1);
        }
    };
    prefixes
        .get(&prefix_name)
        .map(|iri| Term::Literal(Literal::from(iri.to_string())))
}

fn expand_prefixed_name(prefixes: &HashMap<String, String>, qname: &Term) -> Option<Term> {
    let qname_str = match qname {
        Term::Literal(l) => l.value().to_string(),
        _ => {
            eprintln!("Invalid argument passed to expandPrefixedName: {:?}", qname);
            exit(-1);
        }
    };
    let (prefix_name, rest) = qname_str.split_at(match qname_str.find(':') {
        Some(offset) => offset,
        _ => {
            eprintln!("Malformed QName in expandPrefixedName: {:?}", &qname_str);
            return None;
        }
    });
    prefixes.get(prefix_name).map(|pref_iri| {
        Term::NamedNode(NamedNode::new(pref_iri.to_string() + rest.get(1..).unwrap()).unwrap())
    })
}

fn extract_prefixes(query_text: &str) -> HashMap<String, String> {
    let mut prefix_map = HashMap::new();

    let re = Regex::new(r"\b[pP][rR][eE][fF][iI][xX]\s+(\S*?):\s+<(.+?)>").unwrap();
    for (_, [prefix, iri]) in re.captures_iter(query_text).map(|c| c.extract()) {
        prefix_map.insert(String::from(prefix), String::from(iri));
    }
    prefix_map
}

fn extract_variables(query_text: &str) -> HashSet<String> {
    let re = Regex::new(r"\?([A-Za-z_][A-Za-z_0-9]*?)[^A-Za-z_0-9]").unwrap();
    re.captures_iter(query_text)
        .map(|c| c.extract())
        .map(|(_, [varname])| varname.to_string())
        .collect()
}

fn clean_column(field: &str, normalize: &bool) -> String {
    if *normalize {
        field.trim().to_uppercase().replace('\"', "")
    } else {
        field.trim().replace('\"', "")
    }
}

pub fn parse_args<I>(args: I) -> ArgMatches
where
    I: IntoIterator<Item = String>,
{
    command!()
        .about("Convert CSV file to RDF using SPARQL")
        .arg(
            Arg::new("delimiter")
                .short('d')
                .long("delimiter")
                .default_value(",")
                .conflicts_with("tab")
                .help("Delimiting character of the input file"),
        )
        .arg(
            Arg::new("tab")
                .short('t')
                .long("tab")
                .action(ArgAction::SetTrue)
                .conflicts_with("delimiter")
                .help("Is the Input tab-separated (TSV)?"),
        )
        .arg(
            Arg::new("escape_char")
                .short('p')
                .long("escape_char")
                .default_value("\\")
                .help("Escape character used in the input file"),
        )
        .arg(
            Arg::new("quote_char")
                .long("quote_char")
                .default_value("\"")
                .help("Quote character used in the input file"),
        )
        .arg(
            Arg::new("normalize")
                .short('n')
                .long("normalize")
                .action(ArgAction::SetTrue)
                .help(
                    "Normalize column names - convert all to UPPERCASE [default: no normalization]",
                ),
        )
        .arg(
            Arg::new("headers")
                .short('H')
                .long("no-header-row")
                .action(ArgAction::SetFalse)
                .help("File has headers in the first row [default: True]"),
        )
        .arg(
            Arg::new("gzip")
                .short('g')
                .long("gzip")
                .action(ArgAction::SetTrue)
                .requires("output")
                .help("gzip file output. Output file name must be provided"),
        )
        .arg(
            Arg::new("ntriples")
                .long("ntriples")
                .action(ArgAction::SetTrue)
                .help("Emit N-Triples [default: turtle]"),
        )
        .arg(
            Arg::new("quads")
                .long("quads")
                .requires("name")
                .action(ArgAction::SetTrue)
                .help("Output quads (trig). Use --name for graph URI"),
        )
        .group(ArgGroup::new("types").args(["ntriples", "quads"]))
        .arg(
            Arg::new("name")
                .long("name")
                .action(ArgAction::Set)
                .default_value("")
                .help("Named graph URI "),
        )
        .arg(
            Arg::new("test")
                .long("test")
                .value_parser(value_parser!(u32).range(1..50))
                .action(ArgAction::Set)
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("5")
                .help("Show output for first TEST records (default=5)"),
        )
        .arg(
            Arg::new("split")
                .long("split")
                .action(ArgAction::Append)
                .num_args(3)
                .value_names(["ORIGINAL", "SPLIT", "DELIMITER"])
                .help("Split column ORIGINAL into multiple values in SPLIT on DELIMITER"),
        )
        .arg(
            Arg::new("dedup")
                .long("dedup")
                .value_parser(value_parser!(u32).range(1000..=5000000))
                .default_missing_value("1000")
                .num_args(0..=1)
                .require_equals(true)
                .action(ArgAction::Set)
                .help("Window size in which to remove duplicate triples (default=1000)"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .action(ArgAction::Set)
                .default_value("STDOUT")
                .help("File to write to, omit to use STDOUT"),
        )
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .action(ArgAction::Set)
                .default_value("STDIN")
                .help("CSV to be processed, omit to use STDIN"),
        )
        .arg(
            Arg::new("query")
                .short('q')
                .long("query")
                .action(ArgAction::Set)
                .required(true)
                .help("File containing a SPARQL query to be applied to an input file (required)"),
        )
        .get_matches_from(args)
}

pub fn configure_transform<I>(args: I) -> OxiTarql
where
    I: IntoIterator<Item = String>,
{
    let matches = parse_args(args);

    let split_def = match matches.get_many::<String>("split") {
        None => vec![],
        Some(splits) => {
            let mut sval_it = splits.cloned();
            let mut split_defs = Vec::<(String, String, String)>::new();
            while let Some(orig) = sval_it.next() {
                split_defs.push((orig, sval_it.next().unwrap(), sval_it.next().unwrap()));
            }
            split_defs
        }
    };

    OxiTarql {
        delimiter: matches.get_one::<String>("delimiter").unwrap().to_string(),
        tab: matches.get_flag("tab"),
        test: match matches.get_one::<u32>("test") {
            None => 0,
            Some(t) => *t,
        },
        headers: matches.get_flag("headers"),
        escape_char: matches
            .get_one::<String>("escape_char")
            .unwrap()
            .to_string(),
        quote_char: matches.get_one::<String>("quote_char").unwrap().to_string(),
        normalize: matches.get_flag("normalize"),
        gzip: matches.get_flag("gzip"),
        quads: matches.get_flag("quads"),
        ntriples: matches.get_flag("ntriples"),
        dedup: match matches.get_one::<u32>("dedup") {
            None => 0,
            Some(t) => *t,
        },
        named_graph: matches.get_one::<String>("name").unwrap().to_string(),
        input: matches.get_one::<String>("input").unwrap().to_string(),
        output: matches.get_one::<String>("output").unwrap().to_string(),
        query: matches.get_one::<String>("query").unwrap().to_string(),
        split: split_def,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_column_no_normalize() {
        assert_eq!(clean_column("  column_name  ", &false), "column_name");
        assert_eq!(clean_column("\"quoted\"", &false), "quoted");
        assert_eq!(clean_column("  \"spaces\"  ", &false), "spaces");
        assert_eq!(clean_column("MixedCase", &false), "MixedCase");
    }

    #[test]
    fn test_clean_column_normalize() {
        assert_eq!(clean_column("  column_name  ", &true), "COLUMN_NAME");
        assert_eq!(clean_column("\"quoted\"", &true), "QUOTED");
        assert_eq!(clean_column("MixedCase", &true), "MIXEDCASE");
        assert_eq!(clean_column("lower", &true), "LOWER");
    }

    #[test]
    fn test_extract_prefixes_basic() {
        let query = r#"
            PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
            PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
            SELECT * WHERE { ?s ?p ?o }
        "#;
        let prefixes = extract_prefixes(query);
        assert_eq!(prefixes.len(), 2);
        assert_eq!(
            prefixes.get("rdf"),
            Some(&"http://www.w3.org/1999/02/22-rdf-syntax-ns#".to_string())
        );
        assert_eq!(
            prefixes.get("rdfs"),
            Some(&"http://www.w3.org/2000/01/rdf-schema#".to_string())
        );
    }

    #[test]
    fn test_extract_prefixes_case_insensitive() {
        let query = r#"
            prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
            PREFIX foaf: <http://xmlns.com/foaf/0.1/>
            PrEfIx dc: <http://purl.org/dc/elements/1.1/>
        "#;
        let prefixes = extract_prefixes(query);
        assert_eq!(prefixes.len(), 3);
        assert!(prefixes.contains_key("rdf"));
        assert!(prefixes.contains_key("foaf"));
        assert!(prefixes.contains_key("dc"));
    }

    #[test]
    fn test_extract_prefixes_empty() {
        let query = "SELECT * WHERE { ?s ?p ?o }";
        let prefixes = extract_prefixes(query);
        assert_eq!(prefixes.len(), 0);
    }

    #[test]
    fn test_extract_variables_basic() {
        let query = "SELECT ?name ?age WHERE { ?person foaf:name ?name . ?person foaf:age ?age }";
        let vars = extract_variables(query);
        assert!(vars.contains("name"));
        assert!(vars.contains("age"));
        assert!(vars.contains("person"));
    }

    #[test]
    fn test_extract_variables_underscores() {
        let query = "SELECT ?first_name ?last_name WHERE { ?s ?p ?o }";
        let vars = extract_variables(query);
        assert!(vars.contains("first_name"));
        assert!(vars.contains("last_name"));
        assert!(vars.contains("s"));
    }

    #[test]
    fn test_extract_variables_with_numbers() {
        let query = "SELECT ?var1 ?var2 ?var123 WHERE { ?s ?p ?o }";
        let vars = extract_variables(query);
        assert!(vars.contains("var1"));
        assert!(vars.contains("var2"));
        assert!(vars.contains("var123"));
    }

    #[test]
    fn test_extract_variables_empty() {
        let query = "SELECT * WHERE { <http://example.org/s> <http://example.org/p> <http://example.org/o> }";
        let vars = extract_variables(query);
        // Should be empty or nearly empty since no variables are used
        assert!(!vars.contains("SELECT"));
        assert!(!vars.contains("WHERE"));
    }

    #[test]
    fn test_expand_prefix_valid() {
        let mut prefixes = HashMap::new();
        prefixes.insert(
            "rdf".to_string(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#".to_string(),
        );
        prefixes.insert("foaf".to_string(), "http://xmlns.com/foaf/0.1/".to_string());

        let prefix_term = Term::Literal(Literal::from("rdf"));
        let result = expand_prefix(&prefixes, &prefix_term);
        assert!(result.is_some());
        if let Some(Term::Literal(lit)) = result {
            assert_eq!(lit.value(), "http://www.w3.org/1999/02/22-rdf-syntax-ns#");
        } else {
            panic!("Expected literal term");
        }
    }

    #[test]
    fn test_expand_prefix_unknown_prefix() {
        let prefixes = HashMap::new();
        let prefix_term = Term::Literal(Literal::from("unknown"));
        let result = expand_prefix(&prefixes, &prefix_term);
        assert!(result.is_none());
    }

    #[test]
    fn test_expand_prefixed_name_valid() {
        let mut prefixes = HashMap::new();
        prefixes.insert(
            "rdf".to_string(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#".to_string(),
        );
        prefixes.insert("foaf".to_string(), "http://xmlns.com/foaf/0.1/".to_string());

        let qname = Term::Literal(Literal::from("foaf:name"));
        let result = expand_prefixed_name(&prefixes, &qname);
        assert!(result.is_some());
        if let Some(Term::NamedNode(node)) = result {
            assert_eq!(node.as_str(), "http://xmlns.com/foaf/0.1/name");
        } else {
            panic!("Expected named node term");
        }
    }

    #[test]
    fn test_expand_prefixed_name_rdf_type() {
        let mut prefixes = HashMap::new();
        prefixes.insert(
            "rdf".to_string(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#".to_string(),
        );

        let qname = Term::Literal(Literal::from("rdf:type"));
        let result = expand_prefixed_name(&prefixes, &qname);
        assert!(result.is_some());
        if let Some(Term::NamedNode(node)) = result {
            assert_eq!(
                node.as_str(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
            );
        } else {
            panic!("Expected named node term");
        }
    }

    #[test]
    fn test_expand_prefixed_name_no_prefix() {
        let prefixes = HashMap::new();
        let qname = Term::Literal(Literal::from("foaf:name"));
        let result = expand_prefixed_name(&prefixes, &qname);
        assert!(result.is_none());
    }

    #[test]
    fn test_expand_prefixed_name_no_colon() {
        let mut prefixes = HashMap::new();
        prefixes.insert(
            "rdf".to_string(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#".to_string(),
        );

        let qname = Term::Literal(Literal::from("nocolon"));
        let result = expand_prefixed_name(&prefixes, &qname);
        assert!(result.is_none());
    }

    #[test]
    fn test_apply_split_no_split() {
        let tarql = OxiTarql {
            split: vec![],
            ..Default::default()
        };
        let headers = vec!["col1".to_string(), "col2".to_string()];
        let record = csv::StringRecord::from(vec!["value1", "value2"]);

        let result = tarql.apply_split(&record, &headers);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2);
        assert_eq!(result[0][0], ("col1".to_string(), "value1"));
        assert_eq!(result[0][1], ("col2".to_string(), "value2"));
    }

    #[test]
    fn test_apply_split_single_split() {
        let tarql = OxiTarql {
            split: vec![("tags".to_string(), "tag".to_string(), ",".to_string())],
            ..Default::default()
        };
        let headers = vec!["name".to_string(), "tags".to_string()];
        let record = csv::StringRecord::from(vec!["Alice", "rust,python,go"]);

        let result = tarql.apply_split(&record, &headers);
        assert_eq!(result.len(), 3); // 3 tags split

        // Check first row
        assert_eq!(result[0][0], ("name".to_string(), "Alice"));
        assert_eq!(result[0][1], ("tags".to_string(), "rust,python,go"));
        assert_eq!(result[0][2], ("tag".to_string(), "rust"));

        // Check second row
        assert_eq!(result[1][0], ("name".to_string(), "Alice"));
        assert_eq!(result[1][2], ("tag".to_string(), "python"));

        // Check third row
        assert_eq!(result[2][2], ("tag".to_string(), "go"));
    }

    #[test]
    fn test_apply_split_multiple_splits() {
        let tarql = OxiTarql {
            split: vec![
                ("colors".to_string(), "color".to_string(), ",".to_string()),
                ("sizes".to_string(), "size".to_string(), ";".to_string()),
            ],
            ..Default::default()
        };
        let headers = vec![
            "name".to_string(),
            "colors".to_string(),
            "sizes".to_string(),
        ];
        let record = csv::StringRecord::from(vec!["Product", "red,blue", "S;M"]);

        let result = tarql.apply_split(&record, &headers);
        // 2 colors × 2 sizes = 4 combinations
        assert_eq!(result.len(), 4);

        // Verify all have the color and size fields
        for row in result.iter() {
            assert!(row.iter().any(|(k, _)| k == "color"));
            assert!(row.iter().any(|(k, _)| k == "size"));
        }
    }

    #[test]
    fn test_apply_split_nonexistent_column() {
        let tarql = OxiTarql {
            split: vec![(
                "nonexistent".to_string(),
                "split_val".to_string(),
                ",".to_string(),
            )],
            ..Default::default()
        };
        let headers = vec!["col1".to_string(), "col2".to_string()];
        let record = csv::StringRecord::from(vec!["value1", "value2"]);

        let result = tarql.apply_split(&record, &headers);
        // Should return original row since column doesn't exist
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2);
    }

    #[test]
    fn test_flush_store() {
        use std::io::Cursor;

        let mut store = HashSet::new();
        let triple1 = Triple::new(
            NamedNode::new("http://example.org/s1").unwrap(),
            NamedNode::new("http://example.org/p").unwrap(),
            NamedNode::new("http://example.org/o1").unwrap(),
        );
        let triple2 = Triple::new(
            NamedNode::new("http://example.org/s2").unwrap(),
            NamedNode::new("http://example.org/p").unwrap(),
            NamedNode::new("http://example.org/o2").unwrap(),
        );
        store.insert(triple1);
        store.insert(triple2);

        let buffer = Vec::new();
        let cursor = Cursor::new(buffer);
        let mut writer = BufWriter::new(Box::new(cursor) as Box<dyn Write>);

        let result = flush_store(&mut store, &mut writer);
        assert!(result.is_ok());
        assert_eq!(store.len(), 0); // Store should be cleared
    }
}
