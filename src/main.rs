use std::ops::Index;
use std::{error::Error, fs};

use oxigraph::model::*;
use oxigraph::sparql::QueryResults;
use regex::Regex;
use spareval::QueryEvaluator;
use spargebra::SparqlParser;

use clap::{Arg, ArgAction, ArgGroup, ArgMatches, command, value_parser};
use csv::ReaderBuilder;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write, stdout};
use std::process::exit;
use std::time::Instant;

#[allow(dead_code)]
#[derive(Default)]
pub struct OxiTarql {
    delimiter: String,
    tab: bool,
    test: u32,
    headers: bool,
    escape_char: String,
    quote_char: String,
    normalise: bool,
    gzip: bool,
    ntriples: bool,
    quads: bool,
    dedup: u32,
    named_graph: String,
    input: String,
    output: String,
    query: String
}

impl OxiTarql {
    fn transform(&mut self) -> Result<(), Box<dyn Error>> {
        let start = Instant::now();
        // Build the CSV reader and iterate over each record.
        // println!("Query {:?}", query_path);
        // println!("Input {:?}", input);
        let mut _row = 0;

        let store = Dataset::new();
        
        let query_str = fs::read_to_string(&self.query).unwrap();
        let query = match SparqlParser::new()
            .with_prefix("tarql", "https://semanticarts.com/tarql/")?
            .parse_query(&query_str) {
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
                    move |args| {
                        args.get(0)
                            .map(|p| expand_prefix(&prefixes, &p)
                            .unwrap())
                    }
                )
                .with_custom_function(
                    NamedNode::new("https://semanticarts.com/tarql/expandPrefixedName")?,
                    move |args| {
                        args.get(0)
                            .map(|p| expand_prefixed_name(&p2, &p)
                            .unwrap())
                    }
                );
        let query_vars = extract_variables(&query_str);

        let file = BufReader::with_capacity(100000, File::open(&self.input)?);

        let mut rdr = ReaderBuilder::new()
            .has_headers(self.headers)
            .delimiter(match self.tab{
                true => '\t' as u8,
                _ => self.delimiter.chars().next().unwrap() as u8
            })
            .quote(self.quote_char.chars().next().unwrap() as u8)
            .escape(Some(self.escape_char.chars().next().unwrap() as u8))
            .from_reader(file);

        let mut headers = Vec::new();

        if self.headers {
            let header = rdr.headers()?.clone();

            for field in &header {
                headers.push(clean_column(field, &self.normalise).to_string());
            }
        } else {
            let alphabet_column_names: Vec<String> = ('a'..='z')
                .chain('A'..='Z')
                .map(|c| c.to_string())
                .collect();

            headers = alphabet_column_names.clone();
        }

        for result in rdr.records() {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = match result {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error reading row {}: {:?}", _row, e);
                    exit(-1);
                }
            };
            // println!("{:?}", headers);
            // println!("{:?}", record);
            let mut prepared = evaluator.prepare(&query);
            for (varname, value) in headers.iter().zip(record.iter()) {
                if query_vars.contains(varname) {
                    prepared =
                        prepared.substitute_variable(Variable::new(varname)?, Literal::from(value));
                }
            }
            if query_vars.contains("ROWNUM") {
                prepared = prepared.substitute_variable(Variable::new("ROWNUM")?, Literal::from(_row));
            }

            let results = prepared.execute(&store);
            if let QueryResults::Graph(triples) = results.unwrap() {
                for triple in triples {
                    let _ = writeln!(out_writer, "{}", triple?);
                }
            }

            _row += 1;
            if self.test != 0 && _row == self.test {
                break;
            }
        }

        out_writer.flush().expect("Error flushing to output file");
        Ok(())
    }

}

fn expand_prefix(prefixes: &HashMap<String, String>, prefix: &Term) -> Option<Term> {
    let prefix_name = match prefix {
        Term::Literal(l) => l.value().to_string(),
        _ => {
            eprintln!("Invalid argument passed to expandPrefix: {:?}", prefix);
            exit(-1);
        }
    };
    let expanded = prefixes.get(&prefix_name).map(|iri| Term::Literal(Literal::from(iri.to_string())));
    expanded
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
            return None
        }
    });
    prefixes.get(prefix_name)
        .map(|pref_iri| Term::NamedNode(
            NamedNode::new(pref_iri.to_string() + rest).unwrap()
        )
    )
}

// fn replace_iri_with_prefix(item: &String, prefix_map: &HashMap<String, &str>) -> String {
//     let mut new_item = item.clone();
//     let typed_value: String;

//     if new_item.contains("^^") {
//         let tv = new_item.split("^^").collect::<Vec<&str>>();
//         typed_value = format!("{}^^", tv[0].to_string());
//         new_item = tv[1].to_string();
//     } else {
//         typed_value = "".to_string();
//     }

//     // check if this is IRI that can be replaced with a prefix
//     if new_item.contains("<") && new_item.contains("#") {
//         new_item = new_item.replace("<", "").replace(">", "");
//         let sp = new_item.split("#").collect::<Vec<&str>>();
//         if prefix_map.contains_key(&sp[0].to_string()) {
//             new_item = format!(
//                 "{}{}{}",
//                 typed_value,
//                 prefix_map.get(&sp[0].to_string()).unwrap().to_string(),
//                 sp[1]
//             );
//         }
//     }

//     new_item
// }

fn extract_prefixes(query_text: &String) -> HashMap<String, String> {
    let mut prefix_map = HashMap::new();

    let re = Regex::new(r"\b[pP][rR][eE][fF][iI][xX]\s+(\S*?):\s+<(.+?)>").unwrap();
    for (_, [prefix, iri]) in re.captures_iter(query_text).map(|c| c.extract()) {
        prefix_map.insert(String::from(prefix), String::from(iri));
    }
    prefix_map
}

fn extract_variables(query_text: &String) -> HashSet<String> {
    let re = Regex::new(r"\?([A-Za-z_][A-Za-z_0-9]*?)[^A-Za-z_0-9]").unwrap();
    re.captures_iter(query_text)
        .map(|c| c.extract())
        .map(|(_, [varname])| varname.to_string())
        .collect()
}

fn clean_column(field: &str, normalise: &bool) -> String {
    if *normalise {
        field.trim().to_uppercase().replace('\"', "")
    } else {
        field.trim().replace('\"', "")
    }
}

fn parse_args() -> ArgMatches {
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
            Arg::new("normalise")
                .short('n')
                .long("normalise")
                .action(ArgAction::SetTrue)
                .help(
                    "Normalise column names - convert all to UPPERCASE [default: no normalisation]",
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
        .get_matches()
}

fn main() {
    // parse the supplied arguments
    let matches = parse_args();

    let mut tarql = OxiTarql {
        delimiter: matches.get_one::<String>("delimiter").unwrap().to_string(),
        tab: matches.get_flag("tab"),
        test: match matches.get_one::<u32>("test") {
            None => 0,
            Some(t) => *t
        },
        headers: matches.get_flag("headers"),
        escape_char: matches
            .get_one::<String>("escape_char")
            .unwrap()
            .to_string(),
        quote_char: matches.get_one::<String>("quote_char").unwrap().to_string(),
        normalise: matches.get_flag("normalise"),
        gzip: matches.get_flag("gzip"),
        quads: matches.get_flag("quads"),
        ntriples: matches.get_flag("ntriples"),
        dedup: match matches.get_one::<u32>("dedup") {
            None => 0,
            Some(t) => *t
        },
        named_graph: matches.get_one::<String>("name").unwrap().to_string(),
        input: matches.get_one::<String>("input").unwrap().to_string(),
        output: matches.get_one::<String>("output").unwrap().to_string(),
        query: matches.get_one::<String>("query").unwrap().to_string(),
    };

    let start = Instant::now();

    tarql.transform().expect("Oops something went wrong");

    let duration = start.duration_since(start);
    eprintln!("Processing complete in {} seconds", duration.as_secs_f32());
}
